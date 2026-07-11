#![deny(unsafe_op_in_unsafe_fn)]

use rigos_core::{Diagnostic, InspectionResult, Observation};
use rigos_machine::MachineContext;
use rigos_miner::{InspectedProcessIdentity, MinerBackend};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

pub const MINER_SCHEMA: &str = "rigos.miner-snapshot/v1";
const API_TIMEOUT: Duration = Duration::from_millis(750);
const API_MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct XmrigBackend {
    pub explicit_executable: Option<PathBuf>,
    pub explicit_config: Option<PathBuf>,
    pub probe_version: bool,
}

impl Default for XmrigBackend {
    fn default() -> Self {
        Self {
            explicit_executable: None,
            explicit_config: None,
            probe_version: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProcessOwnerV1 {
    pub uid: Option<u32>,
    pub systemd_unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfigParseState {
    Absent,
    Valid,
    Malformed,
    Unreadable,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConfigInspectionV1 {
    pub path: Option<String>,
    pub parse_state: ConfigParseState,
    pub pools: Vec<String>,
    pub algorithm: Option<String>,
    pub huge_pages_requested: Option<bool>,
    pub thread_hint: Option<u64>,
    pub log_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum ApiStateV1 {
    NotConfigured,
    Disabled,
    Reachable {
        authentication_configured: bool,
        authentication_result: String,
    },
    WildcardReachable {
        authentication_configured: bool,
        authentication_result: String,
    },
    UnsupportedNonLoopbackEndpoint,
    ConnectionRefused,
    Timeout,
    AuthenticationRejected,
    MalformedResponse,
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MinerSnapshotV1 {
    pub backend: String,
    pub detected: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub executable_path: Option<String>,
    pub version: Observation<String>,
    pub process_owner: Option<ProcessOwnerV1>,
    pub uptime_seconds: Observation<u64>,
    pub config: ConfigInspectionV1,
    pub api: ApiStateV1,
    pub algorithm: Option<String>,
    pub hashrate_hs: Option<f64>,
    pub accepted_shares: Option<u64>,
    pub rejected_shares: Option<u64>,
    pub pool_endpoint: Option<String>,
}

#[derive(Default)]
struct ApiObservation {
    state: Option<ApiStateV1>,
    version: Option<String>,
    algorithm: Option<String>,
    hashrate: Option<f64>,
    accepted: Option<u64>,
    rejected: Option<u64>,
    pool: Option<String>,
}

impl MinerBackend for XmrigBackend {
    type Snapshot = MinerSnapshotV1;

    fn discover(&self, machine: &MachineContext) -> InspectionResult<Self::Snapshot> {
        let mut diagnostics = Vec::new();
        let process = discover_process(&machine.proc_root, &mut diagnostics);
        let pid = process.map(InspectedProcessIdentity::pid);
        let cmdline = pid.and_then(|id| read_cmdline(&machine.proc_root, id, &mut diagnostics));
        let executable = pid
            .and_then(|id| fs::read_link(machine.proc_root.join(id.to_string()).join("exe")).ok())
            .or_else(|| self.explicit_executable.clone());
        let config_path = cmdline
            .as_deref()
            .and_then(extract_config_path)
            .or_else(|| self.explicit_config.clone());
        let (config, raw_config) = inspect_config(config_path.as_deref(), &mut diagnostics);
        let mut api = inspect_api(raw_config.as_ref(), &mut diagnostics);
        let version = if let Some(v) = api.version.take() {
            Observation::Observed {
                value: v,
                source: "xmrig_api".into(),
            }
        } else if self.probe_version {
            executable
                .as_deref()
                .map(|p| version_probe::probe(p, pid, &mut diagnostics))
                .unwrap_or(Observation::Unknown)
        } else {
            Observation::Unknown
        };
        let process_owner = pid.map(|id| inspect_owner(&machine.proc_root, id));
        let uptime = pid
            .map(|id| inspect_uptime(&machine.proc_root, id))
            .unwrap_or(Observation::Unknown);
        let running = pid.is_some();
        InspectionResult::success(
            MinerSnapshotV1 {
                backend: "xmrig".into(),
                detected: running || executable.is_some(),
                running,
                pid,
                executable_path: executable.map(|p| p.to_string_lossy().into_owned()),
                version,
                process_owner,
                uptime_seconds: uptime,
                config,
                api: api.state.unwrap_or(ApiStateV1::NotConfigured),
                algorithm: api.algorithm,
                hashrate_hs: api.hashrate.filter(|v| v.is_finite()),
                accepted_shares: api.accepted,
                rejected_shares: api.rejected,
                pool_endpoint: api.pool,
            },
            diagnostics,
        )
    }
}

fn discover_process(
    proc_root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<InspectedProcessIdentity> {
    let entries = match fs::read_dir(proc_root) {
        Ok(v) => v,
        Err(_) => {
            diagnostics.push(Diagnostic::error(
                "xmrig.proc.unavailable",
                "xmrig_process",
                "Process metadata is unavailable.",
            ));
            return None;
        }
    };
    let mut pids: Vec<u32> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse().ok())
        .collect();
    pids.sort_unstable();
    for pid in pids {
        let Ok(comm) = fs::read_to_string(proc_root.join(pid.to_string()).join("comm")) else {
            continue;
        };
        if comm.trim().eq_ignore_ascii_case("xmrig") {
            return InspectedProcessIdentity::new(pid);
        }
    }
    None
}

fn read_cmdline(root: &Path, pid: u32, diagnostics: &mut Vec<Diagnostic>) -> Option<Vec<String>> {
    match fs::read(root.join(pid.to_string()).join("cmdline")) {
        Ok(raw) => Some(
            raw.split(|b| *b == 0)
                .filter(|v| !v.is_empty())
                .map(|v| String::from_utf8_lossy(v).into_owned())
                .collect(),
        ),
        Err(e) => {
            diagnostics.push(Diagnostic::warning(
                "xmrig.process.cmdline_unavailable",
                "xmrig_process",
                format!("XMRig command line is unavailable: {}.", e.kind()),
            ));
            None
        }
    }
}

pub fn extract_config_path(args: &[String]) -> Option<PathBuf> {
    for (index, arg) in args.iter().enumerate() {
        if arg == "-c" {
            return args
                .get(index + 1)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from);
        }
        if arg == "--config" {
            return args
                .get(index + 1)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--config=").filter(|v| !v.is_empty()) {
            return Some(path.into());
        }
    }
    None
}

fn inspect_owner(root: &Path, pid: u32) -> ProcessOwnerV1 {
    let dir = root.join(pid.to_string());
    let uid = fs::read_to_string(dir.join("status")).ok().and_then(|s| {
        s.lines()
            .find(|l| l.starts_with("Uid:"))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    });
    let systemd_unit = fs::read_to_string(dir.join("cgroup")).ok().and_then(|s| {
        s.lines().find_map(|line| {
            line.split('/')
                .find(|part| part.ends_with(".service"))
                .map(str::to_owned)
        })
    });
    ProcessOwnerV1 { uid, systemd_unit }
}

fn inspect_uptime(root: &Path, pid: u32) -> Observation<u64> {
    let Some(stat) = fs::read_to_string(root.join(pid.to_string()).join("stat")).ok() else {
        return Observation::Unavailable {
            reason: "process_stat_unavailable".into(),
        };
    };
    let Some(end) = stat.rfind(')') else {
        return Observation::Unavailable {
            reason: "malformed_process_stat".into(),
        };
    };
    let start_ticks: u64 = match stat[end + 1..]
        .split_whitespace()
        .nth(19)
        .and_then(|v| v.parse().ok())
    {
        Some(v) => v,
        None => {
            return Observation::Unavailable {
                reason: "malformed_process_stat".into(),
            };
        }
    };
    let system_uptime: f64 = match fs::read_to_string(root.join("uptime"))
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
    {
        Some(v) => v,
        None => {
            return Observation::Unavailable {
                reason: "system_uptime_unavailable".into(),
            };
        }
    };
    let hz = clock_ticks_per_second().unwrap_or(100);
    Observation::Observed {
        value: (system_uptime as u64).saturating_sub(start_ticks / hz),
        source: "proc_stat".into(),
    }
}

#[cfg(unix)]
fn clock_ticks_per_second() -> Option<u64> {
    // SAFETY: sysconf is read-only, takes a constant selector, and touches no caller memory.
    let value = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    (value > 0).then_some(value as u64)
}

#[cfg(not(unix))]
fn clock_ticks_per_second() -> Option<u64> {
    None
}

fn inspect_config(
    path: Option<&Path>,
    diagnostics: &mut Vec<Diagnostic>,
) -> (ConfigInspectionV1, Option<Value>) {
    let Some(path) = path else {
        return (
            ConfigInspectionV1 {
                path: None,
                parse_state: ConfigParseState::Absent,
                pools: vec![],
                algorithm: None,
                huge_pages_requested: None,
                thread_hint: None,
                log_path: None,
            },
            None,
        );
    };
    let display = Some(path.to_string_lossy().into_owned());
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(e) => {
            diagnostics.push(Diagnostic::warning(
                "xmrig.config.unreadable",
                "xmrig_config",
                format!("XMRig configuration is unreadable: {}.", e.kind()),
            ));
            return (
                ConfigInspectionV1 {
                    path: display,
                    parse_state: ConfigParseState::Unreadable,
                    pools: vec![],
                    algorithm: None,
                    huge_pages_requested: None,
                    thread_hint: None,
                    log_path: None,
                },
                None,
            );
        }
    };
    let raw: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => {
            diagnostics.push(Diagnostic::warning(
                "xmrig.config.malformed_json",
                "xmrig_config",
                "XMRig configuration contains malformed JSON.",
            ));
            return (
                ConfigInspectionV1 {
                    path: display,
                    parse_state: ConfigParseState::Malformed,
                    pools: vec![],
                    algorithm: None,
                    huge_pages_requested: None,
                    thread_hint: None,
                    log_path: None,
                },
                None,
            );
        }
    };
    let pools_array = raw.get("pools").and_then(Value::as_array);
    let pools = pools_array
        .into_iter()
        .flatten()
        .filter_map(|v| v.get("url")?.as_str())
        .map(redact_url)
        .collect();
    let algorithm = raw
        .get("algo")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            pools_array?
                .iter()
                .filter_map(|pool| pool.get("algo")?.as_str())
                .find(|value| !value.is_empty())
                .map(str::to_owned)
        });
    let huge_pages_requested = raw
        .get("cpu")
        .and_then(|v| v.get("huge-pages"))
        .and_then(Value::as_bool)
        .or_else(|| {
            raw.get("randomx")
                .and_then(|v| v.get("huge-pages"))
                .and_then(Value::as_bool)
        });
    let thread_hint = raw
        .get("cpu")
        .and_then(|v| v.get("max-threads-hint"))
        .and_then(Value::as_u64)
        .or_else(|| raw.get("threads").and_then(Value::as_u64));
    let config = ConfigInspectionV1 {
        path: display,
        parse_state: ConfigParseState::Valid,
        pools,
        algorithm,
        huge_pages_requested,
        thread_hint,
        log_path: raw
            .get("log-file")
            .and_then(Value::as_str)
            .map(str::to_owned),
    };
    (config, Some(raw))
}

fn redact_url(value: &str) -> String {
    value
        .rsplit_once('@')
        .map(|(_, host)| host.to_owned())
        .unwrap_or_else(|| value.to_owned())
}

fn inspect_api(config: Option<&Value>, diagnostics: &mut Vec<Diagnostic>) -> ApiObservation {
    let Some(http) = config.and_then(|v| v.get("http")) else {
        return ApiObservation {
            state: Some(ApiStateV1::NotConfigured),
            ..Default::default()
        };
    };
    if http.get("enabled").and_then(Value::as_bool) != Some(true) {
        return ApiObservation {
            state: Some(ApiStateV1::Disabled),
            ..Default::default()
        };
    }
    let host = http
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or("127.0.0.1");
    let Some(port) = http
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| *v > 0)
    else {
        diagnostics.push(Diagnostic::warning(
            "xmrig.api.invalid_port",
            "xmrig_api",
            "The configured XMRig API port is invalid.",
        ));
        return ApiObservation {
            state: Some(ApiStateV1::Unavailable {
                reason: "invalid_port".into(),
            }),
            ..Default::default()
        };
    };
    let wildcard = host == "0.0.0.0" || host == "::";
    let addresses = match validated_addresses(host, port) {
        Ok(v) => v,
        Err(reason) => {
            diagnostics.push(Diagnostic::warning(
                "xmrig.api.non_loopback_rejected",
                "xmrig_api",
                "The XMRig API endpoint is not an approved loopback address.",
            ));
            return ApiObservation {
                state: Some(ApiStateV1::UnsupportedNonLoopbackEndpoint),
                ..Default::default()
            }
            .with_reason(reason);
        }
    };
    if wildcard {
        diagnostics.push(Diagnostic::warning(
            "xmrig.api.wildcard_exposure",
            "xmrig_api",
            "XMRig API uses a wildcard bind and may be exposed beyond loopback.",
        ));
    }
    let token = http
        .get("access-token")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty());
    match fetch_api(addresses[0], token) {
        Ok((status, _)) if status == 401 || status == 403 => ApiObservation {
            state: Some(ApiStateV1::AuthenticationRejected),
            ..Default::default()
        },
        Ok((status, _)) if status != 200 => ApiObservation {
            state: Some(ApiStateV1::MalformedResponse),
            ..Default::default()
        },
        Ok((_, value)) => parse_api(value, token.is_some(), wildcard),
        Err(ApiError::Timeout) => ApiObservation {
            state: Some(ApiStateV1::Timeout),
            ..Default::default()
        },
        Err(ApiError::Refused) => ApiObservation {
            state: Some(ApiStateV1::ConnectionRefused),
            ..Default::default()
        },
        Err(ApiError::Malformed) => ApiObservation {
            state: Some(ApiStateV1::MalformedResponse),
            ..Default::default()
        },
        Err(ApiError::Io) => ApiObservation {
            state: Some(ApiStateV1::Unavailable {
                reason: "io_error".into(),
            }),
            ..Default::default()
        },
    }
}

impl ApiObservation {
    fn with_reason(self, _reason: String) -> Self {
        self
    }
}

fn validated_addresses(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    if host == "0.0.0.0" {
        return Ok(vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)]);
    }
    if host == "::" {
        return Ok(vec![SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port)]);
    }
    let numeric = host.parse::<IpAddr>().ok();
    if numeric.is_none() && !host.eq_ignore_ascii_case("localhost") {
        return Err("hostname_not_allowed".into());
    }
    let mut addresses: Vec<_> = if let Some(ip) = numeric {
        vec![SocketAddr::new(ip, port)]
    } else {
        let host = host.to_owned();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = (host.as_str(), port)
                .to_socket_addrs()
                .map(|values| values.collect::<Vec<_>>());
            let _ = sender.send(result);
        });
        receiver
            .recv_timeout(Duration::from_millis(250))
            .map_err(|_| "resolution_timeout")?
            .map_err(|_| "resolution_failed")?
    };
    if addresses.is_empty() || addresses.iter().any(|a| !a.ip().is_loopback()) {
        return Err("non_loopback_address".into());
    }
    addresses.sort();
    addresses.dedup();
    Ok(addresses)
}

enum ApiError {
    Timeout,
    Refused,
    Malformed,
    Io,
}

fn fetch_api(addr: SocketAddr, token: Option<&str>) -> Result<(u16, Value), ApiError> {
    let mut stream = TcpStream::connect_timeout(&addr, API_TIMEOUT).map_err(classify_io)?;
    stream
        .set_read_timeout(Some(API_TIMEOUT))
        .map_err(|_| ApiError::Io)?;
    stream
        .set_write_timeout(Some(API_TIMEOUT))
        .map_err(|_| ApiError::Io)?;
    let auth = token
        .map(|v| format!("Authorization: Bearer {v}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "GET /2/summary HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nAccept: application/json\r\n{auth}\r\n"
    );
    stream.write_all(request.as_bytes()).map_err(classify_io)?;
    let mut bounded = stream.take((API_MAX_BYTES + 1) as u64);
    let mut bytes = Vec::new();
    bounded.read_to_end(&mut bytes).map_err(classify_io)?;
    if bytes.len() > API_MAX_BYTES {
        return Err(ApiError::Malformed);
    }
    let split = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or(ApiError::Malformed)?;
    let head = std::str::from_utf8(&bytes[..split]).map_err(|_| ApiError::Malformed)?;
    if head
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("location:"))
    {
        return Err(ApiError::Malformed);
    }
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .ok_or(ApiError::Malformed)?;
    let value = serde_json::from_slice(&bytes[split + 4..]).map_err(|_| ApiError::Malformed)?;
    Ok((status, value))
}

fn classify_io(e: io::Error) -> ApiError {
    match e.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ApiError::Timeout,
        io::ErrorKind::ConnectionRefused => ApiError::Refused,
        _ => ApiError::Io,
    }
}

fn parse_api(value: Value, auth: bool, wildcard: bool) -> ApiObservation {
    let hashrate = value
        .pointer("/hashrate/total/0")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite());
    let state = if wildcard {
        ApiStateV1::WildcardReachable {
            authentication_configured: auth,
            authentication_result: "accepted".into(),
        }
    } else {
        ApiStateV1::Reachable {
            authentication_configured: auth,
            authentication_result: "accepted".into(),
        }
    };
    ApiObservation {
        state: Some(state),
        version: value
            .pointer("/version")
            .and_then(Value::as_str)
            .map(str::to_owned),
        algorithm: value
            .pointer("/algo")
            .and_then(Value::as_str)
            .map(str::to_owned),
        hashrate,
        accepted: value
            .pointer("/results/shares_good")
            .and_then(Value::as_u64),
        rejected: value
            .pointer("/results/shares_total")
            .and_then(Value::as_u64)
            .zip(
                value
                    .pointer("/results/shares_good")
                    .and_then(Value::as_u64),
            )
            .map(|(t, g)| t.saturating_sub(g)),
        pool: value
            .pointer("/connection/pool")
            .and_then(Value::as_str)
            .map(redact_url),
    }
}

mod version_probe;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn extracts_both_config_forms() {
        assert_eq!(
            extract_config_path(&["xmrig".into(), "-c".into(), "/run/rigos/xmrig.json".into()]),
            Some("/run/rigos/xmrig.json".into())
        );
        assert_eq!(
            extract_config_path(&["xmrig".into(), "--config".into(), "/a.json".into()]),
            Some("/a.json".into())
        );
        assert_eq!(
            extract_config_path(&["xmrig".into(), "--config=/b.json".into()]),
            Some("/b.json".into())
        );
        assert_eq!(extract_config_path(&["xmrig".into(), "-c".into()]), None);
        assert_eq!(
            extract_config_path(&["xmrig".into(), "-c".into(), "".into()]),
            None
        );
    }

    #[test]
    fn loopback_policy_rejects_lan() {
        assert!(validated_addresses("127.0.0.2", 1).is_ok());
        assert!(validated_addresses("192.168.1.2", 1).is_err());
    }

    #[test]
    fn redacts_url_userinfo() {
        assert_eq!(redact_url("wallet:secret@pool:3333"), "pool:3333");
    }

    #[test]
    fn config_preserves_unknown_fields_in_raw_value_and_redacts_pool_userinfo() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rigos-xmrig-{unique}.json"));
        fs::write(&path, r#"{"future":{"x":1},"pools":[{"url":"wallet:SENTINEL_SECRET@pool:3333"}],"http":{"enabled":false}}"#).unwrap();
        let mut diagnostics = Vec::new();
        let (config, raw) = inspect_config(Some(&path), &mut diagnostics);
        let _ = fs::remove_file(path);
        assert!(diagnostics.is_empty());
        assert_eq!(config.pools, vec!["pool:3333"]);
        assert_eq!(
            raw.unwrap().pointer("/future/x").and_then(Value::as_u64),
            Some(1)
        );
        assert!(
            !serde_json::to_string(&config)
                .unwrap()
                .contains("SENTINEL_SECRET")
        );
    }

    #[test]
    fn generated_rigos_xmrig_schema_is_inspected_without_secret_leakage() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rigos-xmrig-generated-{unique}.json"));
        fs::write(
            &path,
            r#"{"autosave":false,"cpu":{"huge-pages":true,"max-threads-hint":100},"pools":[{"url":"139.99.69.109:10001","user":"SYNTHETIC_IDENTITY","pass":"rig02","algo":"rx/0"}],"http":{"enabled":false}}"#,
        )
        .unwrap();
        let mut diagnostics = Vec::new();
        let (config, raw) = inspect_config(Some(&path), &mut diagnostics);
        let _ = fs::remove_file(path);
        assert!(diagnostics.is_empty());
        assert!(matches!(config.parse_state, ConfigParseState::Valid));
        assert_eq!(config.pools, vec!["139.99.69.109:10001"]);
        assert_eq!(config.algorithm.as_deref(), Some("rx/0"));
        assert_eq!(config.huge_pages_requested, Some(true));
        assert_eq!(config.thread_hint, Some(100));
        let snapshot = serde_json::to_string(&config).unwrap();
        assert!(!snapshot.contains("SYNTHETIC_IDENTITY"));
        assert!(raw.unwrap().pointer("/pools/0/user").is_some());
    }

    #[test]
    fn legacy_xmrig_schema_remains_supported() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rigos-xmrig-legacy-{unique}.json"));
        fs::write(
            &path,
            r#"{"algo":"rx/0","threads":8,"randomx":{"huge-pages":true},"pools":[{"url":"identity:redacted@pool.test:3333"}],"http":{"enabled":false}}"#,
        )
        .unwrap();
        let mut diagnostics = Vec::new();
        let (config, raw) = inspect_config(Some(&path), &mut diagnostics);
        let _ = fs::remove_file(path);
        assert!(diagnostics.is_empty());
        assert_eq!(config.pools, vec!["pool.test:3333"]);
        assert_eq!(config.algorithm.as_deref(), Some("rx/0"));
        assert_eq!(config.huge_pages_requested, Some(true));
        assert_eq!(config.thread_hint, Some(8));
        assert!(raw.is_some());
    }

    #[test]
    fn malformed_config_is_diagnostic_not_panic() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rigos-xmrig-bad-{unique}.json"));
        fs::write(&path, "{").unwrap();
        let mut diagnostics = Vec::new();
        let (config, raw) = inspect_config(Some(&path), &mut diagnostics);
        let _ = fs::remove_file(path);
        assert!(matches!(config.parse_state, ConfigParseState::Malformed));
        assert!(raw.is_none());
        assert_eq!(diagnostics[0].code, "xmrig.config.malformed_json");
    }

    #[test]
    fn discovers_xmrig_from_synthetic_proc_without_mutation() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rigos-proc-{unique}"));
        let proc_root = root.join("proc");
        let pid_dir = proc_root.join("42");
        fs::create_dir_all(&pid_dir).unwrap();
        let config = root.join("config.json");
        fs::write(&config, r#"{"http":{"enabled":false}}"#).unwrap();
        fs::write(pid_dir.join("comm"), "xmrig\n").unwrap();
        fs::write(
            pid_dir.join("cmdline"),
            format!("xmrig\0--config={}\0", config.display()),
        )
        .unwrap();
        fs::write(
            pid_dir.join("status"),
            "Name:\txmrig\nUid:\t1000 1000 1000 1000\n",
        )
        .unwrap();
        fs::write(pid_dir.join("cgroup"), "0::/system.slice/xmrig.service\n").unwrap();
        fs::write(
            pid_dir.join("stat"),
            "42 (xmrig) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 100 0\n",
        )
        .unwrap();
        fs::write(proc_root.join("uptime"), "100.0 0.0\n").unwrap();
        let backend = XmrigBackend {
            explicit_executable: None,
            explicit_config: None,
            probe_version: false,
        };
        let result = backend.discover(&MachineContext {
            proc_root,
            sys_root: root.join("sys"),
        });
        let _ = fs::remove_dir_all(root);
        let snapshot = result.value.unwrap();
        assert!(snapshot.running);
        assert_eq!(snapshot.pid, Some(42));
        assert!(matches!(
            snapshot.config.parse_state,
            ConfigParseState::Valid
        ));
        assert!(matches!(snapshot.api, ApiStateV1::Disabled));
    }
}
