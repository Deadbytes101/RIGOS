#![forbid(unsafe_code)]

use chrono::{SecondsFormat, Utc};
use schemars::JsonSchema;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
pub const MAX_SHEET_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlightSource {
    Native,
    Import,
    Interactive,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MinerStartMode {
    Manual,
    OnBoot,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RigProfile {
    pub node_name: String,
    pub timezone: String,
    pub flight_source: FlightSource,
    pub flight_ref: Option<String>,
    pub miner_start_mode: MinerStartMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Pool {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub priority: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum Threads {
    Auto(String),
    Count(u16),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CpuPolicy {
    pub threads: Threads,
    pub huge_pages: bool,
    pub max_threads_hint: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FlightSheet {
    pub schema: String,
    pub name: String,
    pub coin: String,
    pub backend: String,
    pub algorithm: String,
    pub pools: Vec<Pool>,
    pub identity_ref: String,
    pub worker_template: String,
    pub cpu: CpuPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalReference {
    pub source: String,
    pub external_type: String,
    pub external_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProvenance {
    pub schema: String,
    pub source_type: String,
    pub source_filename: String,
    pub source_sha256: String,
    pub imported_at: String,
    pub warnings: Vec<String>,
    pub external_reference: Option<ExternalReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub schema: String,
    pub profile: RigProfile,
    pub flight_sheet: FlightSheet,
    pub provenance: Option<ImportProvenance>,
    pub source_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IdentityRecord {
    pub schema: String,
    pub alias: String,
    pub kind: String,
    pub value: String,
    pub created_locally: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    pub code: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub key: Option<String>,
    pub message: String,
}

#[derive(Debug, Error)]
#[error("{diagnostic:?}")]
pub struct ConfigError {
    pub diagnostic: ConfigDiagnostic,
}

fn error(
    code: &str,
    file: Option<&str>,
    line: Option<usize>,
    key: Option<&str>,
    message: impl Into<String>,
) -> ConfigError {
    ConfigError {
        diagnostic: ConfigDiagnostic {
            code: code.into(),
            file: file.map(str::to_owned),
            line,
            key: key.map(str::to_owned),
            message: message.into(),
        },
    }
}

pub fn parse_rig_profile(bytes: &[u8]) -> Result<RigProfile, ConfigError> {
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(error(
            "RIGOS_CONFIG_FILE_TOO_LARGE",
            Some("rig.conf"),
            None,
            None,
            "configuration exceeds 64 KiB",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        error(
            "RIGOS_CONFIG_INVALID_UTF8",
            Some("rig.conf"),
            None,
            None,
            "configuration is not valid UTF-8",
        )
    })?;
    let supported = [
        "RIGOS_CONFIG_VERSION",
        "NODE_NAME",
        "TIMEZONE",
        "FLIGHT_SOURCE",
        "FLIGHT_REF",
        "MINER_START_MODE",
    ];
    let reserved = [
        "WATCHDOG_ENABLED",
        "AUTO_START",
        "ACTIVE_FLIGHT_SHEET",
        "IMPORT_FLIGHT_SHEET",
    ];
    let mut values = BTreeMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let value = raw.trim();
        if value.is_empty() || value.starts_with('#') {
            continue;
        }
        if value.contains("$(`") || value.contains("$(") || value.contains('`') {
            return Err(error(
                "RIGOS_CONFIG_SYNTAX",
                Some("rig.conf"),
                Some(line),
                None,
                "shell expansion is forbidden",
            ));
        }
        let (key, field) = value.split_once('=').ok_or_else(|| {
            error(
                "RIGOS_CONFIG_SYNTAX",
                Some("rig.conf"),
                Some(line),
                None,
                "expected KEY=VALUE",
            )
        })?;
        if key.is_empty()
            || !key.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
            || field.trim() != field
            || field.contains(['\r', '\n'])
        {
            return Err(error(
                "RIGOS_CONFIG_SYNTAX",
                Some("rig.conf"),
                Some(line),
                Some(key),
                "invalid key or unquoted value",
            ));
        }
        if reserved.contains(&key) {
            return Err(error(
                "RIGOS_CONFIG_UNSUPPORTED_KEY",
                Some("rig.conf"),
                Some(line),
                Some(key),
                format!("unsupported key {key} in config version 1"),
            ));
        }
        if !supported.contains(&key) {
            return Err(error(
                "RIGOS_CONFIG_UNKNOWN_KEY",
                Some("rig.conf"),
                Some(line),
                Some(key),
                format!("unknown key {key}"),
            ));
        }
        if values
            .insert(key.to_owned(), (field.to_owned(), line))
            .is_some()
        {
            return Err(error(
                "RIGOS_CONFIG_DUPLICATE_KEY",
                Some("rig.conf"),
                Some(line),
                Some(key),
                format!("duplicate key {key}"),
            ));
        }
    }
    let get = |key: &str| {
        values.get(key).map(|v| v.0.as_str()).ok_or_else(|| {
            error(
                "RIGOS_CONFIG_INVALID_VALUE",
                Some("rig.conf"),
                None,
                Some(key),
                format!("missing required key {key}"),
            )
        })
    };
    if get("RIGOS_CONFIG_VERSION")? != "1" {
        return Err(error(
            "RIGOS_CONFIG_VERSION_UNSUPPORTED",
            Some("rig.conf"),
            values.get("RIGOS_CONFIG_VERSION").map(|v| v.1),
            Some("RIGOS_CONFIG_VERSION"),
            "only config version 1 is supported",
        ));
    }
    let node_name = get("NODE_NAME")?.to_owned();
    if !valid_slug(&node_name, 63) {
        return Err(error(
            "RIGOS_CONFIG_INVALID_VALUE",
            Some("rig.conf"),
            values.get("NODE_NAME").map(|v| v.1),
            Some("NODE_NAME"),
            "node name is not hostname safe",
        ));
    }
    let timezone = get("TIMEZONE")?.to_owned();
    if !valid_timezone_name(&timezone) {
        return Err(error(
            "RIGOS_CONFIG_INVALID_VALUE",
            Some("rig.conf"),
            values.get("TIMEZONE").map(|v| v.1),
            Some("TIMEZONE"),
            "invalid timezone name",
        ));
    }
    let flight_source = match get("FLIGHT_SOURCE")? {
        "native" => FlightSource::Native,
        "import" => FlightSource::Import,
        "interactive" => FlightSource::Interactive,
        _ => {
            return Err(error(
                "RIGOS_CONFIG_INVALID_VALUE",
                Some("rig.conf"),
                values.get("FLIGHT_SOURCE").map(|v| v.1),
                Some("FLIGHT_SOURCE"),
                "expected native import or interactive",
            ));
        }
    };
    let flight_ref = values.get("FLIGHT_REF").map(|v| v.0.clone());
    match (&flight_source, &flight_ref) {
        (FlightSource::Interactive, None) => {}
        (FlightSource::Interactive, Some(_)) => {
            return Err(error(
                "RIGOS_CONFIG_INVALID_VALUE",
                Some("rig.conf"),
                values.get("FLIGHT_REF").map(|v| v.1),
                Some("FLIGHT_REF"),
                "interactive source forbids FLIGHT_REF",
            ));
        }
        (_, None) => {
            return Err(error(
                "RIGOS_CONFIG_INVALID_VALUE",
                Some("rig.conf"),
                None,
                Some("FLIGHT_REF"),
                "selected source requires FLIGHT_REF",
            ));
        }
        (FlightSource::Native, Some(name)) if !valid_slug(name, 64) => {
            return Err(error(
                "RIGOS_CONFIG_INVALID_VALUE",
                Some("rig.conf"),
                values.get("FLIGHT_REF").map(|v| v.1),
                Some("FLIGHT_REF"),
                "invalid flight sheet slug",
            ));
        }
        (FlightSource::Import, Some(name)) if !safe_json_basename(name) => {
            return Err(error(
                "RIGOS_CONFIG_INVALID_VALUE",
                Some("rig.conf"),
                values.get("FLIGHT_REF").map(|v| v.1),
                Some("FLIGHT_REF"),
                "invalid import basename",
            ));
        }
        _ => {}
    }
    let miner_start_mode = match get("MINER_START_MODE")? {
        "manual" => MinerStartMode::Manual,
        "on_boot" => MinerStartMode::OnBoot,
        _ => {
            return Err(error(
                "RIGOS_CONFIG_INVALID_VALUE",
                Some("rig.conf"),
                values.get("MINER_START_MODE").map(|v| v.1),
                Some("MINER_START_MODE"),
                "expected manual or on_boot",
            ));
        }
    };
    Ok(RigProfile {
        node_name,
        timezone,
        flight_source,
        flight_ref,
        miner_start_mode,
    })
}

pub fn parse_flight_sheet(bytes: &[u8], filename: &str) -> Result<FlightSheet, ConfigError> {
    if bytes.len() > MAX_SHEET_BYTES {
        return Err(error(
            "RIGOS_CONFIG_FILE_TOO_LARGE",
            Some(filename),
            None,
            None,
            "flight sheet exceeds 1 MiB",
        ));
    }
    let mut sheet: FlightSheet = serde_json::from_slice(bytes).map_err(|_| {
        error(
            "RIGOS_FLIGHT_SHEET_INVALID",
            Some(filename),
            None,
            None,
            "invalid RIGOS flight sheet JSON",
        )
    })?;
    validate_sheet(&mut sheet, filename)?;
    Ok(sheet)
}

fn validate_sheet(sheet: &mut FlightSheet, filename: &str) -> Result<(), ConfigError> {
    if sheet.schema != "rigos.flight-sheet/v1"
        || !valid_slug(&sheet.name, 64)
        || sheet.backend != "xmrig"
        || sheet.coin.is_empty()
        || sheet.coin.len() > 16
        || sheet.algorithm.is_empty()
        || sheet.algorithm.len() > 64
    {
        return Err(error(
            "RIGOS_FLIGHT_SHEET_INVALID",
            Some(filename),
            None,
            None,
            "unsupported schema backend or metadata",
        ));
    }
    if !valid_slug(&sheet.identity_ref, 64)
        || !matches!(
            sheet.worker_template.as_str(),
            "{node_name}" | "{node_name}.rig"
        )
    {
        return Err(error(
            "RIGOS_FLIGHT_SHEET_INVALID",
            Some(filename),
            None,
            Some("identity_ref"),
            "invalid identity reference or worker template",
        ));
    }
    if sheet.pools.is_empty() || sheet.pools.len() > 16 {
        return Err(error(
            "RIGOS_FLIGHT_SHEET_INVALID",
            Some(filename),
            None,
            Some("pools"),
            "one to sixteen pools are required",
        ));
    }
    let mut priorities = BTreeSet::new();
    for pool in &mut sheet.pools {
        pool.host.make_ascii_lowercase();
        if !valid_host(&pool.host) || pool.port == 0 || !priorities.insert(pool.priority) {
            return Err(error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("pools"),
                "invalid host port or duplicate priority",
            ));
        }
    }
    sheet.pools.sort_by_key(|pool| pool.priority);
    match &sheet.cpu.threads {
        Threads::Auto(value) if value == "auto" => {}
        Threads::Count(value) if (1..=1024).contains(value) => {}
        _ => {
            return Err(error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("cpu.threads"),
                "threads must be auto or 1 through 1024",
            ));
        }
    }
    if !(1..=100).contains(&sheet.cpu.max_threads_hint) {
        return Err(error(
            "RIGOS_FLIGHT_SHEET_INVALID",
            Some(filename),
            None,
            Some("cpu.max_threads_hint"),
            "max threads hint must be 1 through 100",
        ));
    }
    Ok(())
}

pub fn import_hive_style(
    bytes: &[u8],
    filename: &str,
) -> Result<(FlightSheet, ImportProvenance), ConfigError> {
    if bytes.len() > MAX_SHEET_BYTES {
        return Err(error(
            "RIGOS_CONFIG_FILE_TOO_LARGE",
            Some(filename),
            None,
            None,
            "import exceeds 1 MiB",
        ));
    }
    let root: Value = serde_json::from_slice(bytes).map_err(|_| {
        error(
            "RIGOS_FLIGHT_SHEET_INVALID",
            Some(filename),
            None,
            None,
            "invalid external JSON",
        )
    })?;
    let object = root.as_object().ok_or_else(|| {
        error(
            "RIGOS_FLIGHT_SHEET_INVALID",
            Some(filename),
            None,
            None,
            "external sheet must be an object",
        )
    })?;
    for forbidden in [
        "hive_host_url",
        "api_host_url",
        "rig_id",
        "farm_id",
        "rig_passwd",
        "hssh_srv",
        "token",
        "password",
    ] {
        if object.keys().any(|key| key.eq_ignore_ascii_case(forbidden)) {
            return Err(error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some(forbidden),
                "dangerous cloud or credential field is forbidden",
            ));
        }
    }
    let miner = string_field(object, &["miner", "miner_name"]).unwrap_or("xmrig");
    if !miner.to_ascii_lowercase().contains("xmrig") {
        return Err(error(
            "RIGOS_FLIGHT_SHEET_INVALID",
            Some(filename),
            None,
            Some("miner"),
            "only XMRig imports are supported",
        ));
    }
    let name = string_field(object, &["name"])
        .unwrap_or("imported-xmrig")
        .to_ascii_lowercase()
        .replace([' ', '_'], "-");
    let coin = string_field(object, &["coin"]).unwrap_or("XMR").to_owned();
    let algorithm = string_field(object, &["algo", "algorithm"])
        .unwrap_or("auto")
        .to_owned();
    if let Some(pass) = string_field(object, &["pass"]) {
        if !matches!(pass, "x" | "%WORKER_NAME%" | "{node_name}") {
            return Err(error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("pass"),
                "embedded pool password is forbidden",
            ));
        }
    }
    let worker_template = match string_field(object, &["template"]) {
        None | Some("%WORKER_NAME%") | Some("{node_name}") => "{node_name}",
        Some("%WORKER_NAME%.rig") | Some("{node_name}.rig") => "{node_name}.rig",
        Some(_) => {
            return Err(error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("template"),
                "unsupported worker placeholder",
            ));
        }
    };
    let urls =
        string_list(object.get("pool_urls").or_else(|| object.get("pools"))).ok_or_else(|| {
            error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("pool_urls"),
                "pool_urls is required",
            )
        })?;
    let tls_values = bool_list(object.get("pool_ssl"), urls.len());
    let mut pools = Vec::new();
    for (index, raw) in urls.iter().enumerate() {
        let clean = raw
            .strip_prefix("stratum+ssl://")
            .or_else(|| raw.strip_prefix("stratum+tcp://"))
            .unwrap_or(raw);
        let (host, port) = clean.rsplit_once(':').ok_or_else(|| {
            error(
                "RIGOS_FLIGHT_SHEET_INVALID",
                Some(filename),
                None,
                Some("pool_urls"),
                "pool endpoint must be host:port",
            )
        })?;
        pools.push(Pool {
            host: host.to_owned(),
            port: port.parse().map_err(|_| {
                error(
                    "RIGOS_FLIGHT_SHEET_INVALID",
                    Some(filename),
                    None,
                    Some("pool_urls"),
                    "invalid pool port",
                )
            })?,
            tls: raw.starts_with("stratum+ssl://")
                || tls_values.get(index).copied().unwrap_or(false),
            priority: index as u16,
        });
    }
    let wal = object.get("wal_id").map(value_as_reference);
    let identity_ref = wal
        .as_deref()
        .map(|v| format!("hive-wal-{v}"))
        .unwrap_or_else(|| "unresolved".into());
    let mut huge_pages = true;
    let mut max_threads_hint = 100;
    for key in ["cpu_config", "user_config"] {
        if let Some(value) = object.get(key) {
            let fragment = if let Some(text) = value.as_str() {
                Value::Object(parse_json_member_fragment(text, filename, key)?)
            } else {
                value.clone()
            };
            let cpu = fragment.get("cpu").and_then(Value::as_object).or_else(|| {
                (key == "cpu_config")
                    .then(|| fragment.as_object())
                    .flatten()
            });
            if let Some(cpu) = cpu {
                if let Some(v) = cpu.get("huge-pages").and_then(Value::as_bool) {
                    huge_pages = v;
                }
                if let Some(v) = cpu.get("max-threads-hint").and_then(Value::as_u64) {
                    max_threads_hint = u8::try_from(v).unwrap_or(0);
                }
            }
        }
    }
    let mut sheet = FlightSheet {
        schema: "rigos.flight-sheet/v1".into(),
        name,
        coin,
        backend: "xmrig".into(),
        algorithm,
        pools,
        identity_ref,
        worker_template: worker_template.into(),
        cpu: CpuPolicy {
            threads: Threads::Auto("auto".into()),
            huge_pages,
            max_threads_hint,
        },
    };
    validate_sheet(&mut sheet, filename)?;
    let digest = hex::encode(Sha256::digest(bytes));
    let provenance = ImportProvenance {
        schema: "rigos.import-provenance/v1".into(),
        source_type: "external-flight-sheet".into(),
        source_filename: filename.into(),
        source_sha256: digest,
        imported_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        warnings: lifecycle_warnings(object),
        external_reference: wal.map(|value| ExternalReference {
            source: "hive-style".into(),
            external_type: "wal_id".into(),
            external_value: value,
        }),
    };
    Ok((sheet, provenance))
}

fn parse_json_member_fragment(
    input: &str,
    filename: &str,
    key: &str,
) -> Result<Map<String, Value>, ConfigError> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return Err(fragment_error(filename, key));
    }
    if trimmed.starts_with('{') {
        return parse_unique_object(trimmed).map_err(|_| fragment_error(filename, key));
    }
    let wrapped = format!("{{{trimmed}}}");
    if let Ok(value) = parse_unique_object(&wrapped) {
        return Ok(value);
    }
    let normalized =
        insert_top_level_member_commas(trimmed).ok_or_else(|| fragment_error(filename, key))?;
    parse_unique_object(&format!("{{{normalized}}}")).map_err(|_| fragment_error(filename, key))
}

fn fragment_error(filename: &str, key: &str) -> ConfigError {
    error(
        "RIGOS_FLIGHT_SHEET_INVALID",
        Some(filename),
        None,
        Some(key),
        "embedded config is not a strict JSON object or member fragment",
    )
}

struct UniqueObject(Map<String, Value>);

impl<'de> Deserialize<'de> for UniqueObject {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct UniqueObjectVisitor;
        impl<'de> Visitor<'de> for UniqueObjectVisitor {
            type Value = UniqueObject;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object with unique keys")
            }
            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut output = Map::new();
                while let Some((key, value)) = access.next_entry::<String, Value>()? {
                    if output.insert(key.clone(), value).is_some() {
                        return Err(de::Error::custom(format!("duplicate member {key}")));
                    }
                }
                Ok(UniqueObject(output))
            }
        }
        deserializer.deserialize_map(UniqueObjectVisitor)
    }
}

fn parse_unique_object(input: &str) -> Result<Map<String, Value>, serde_json::Error> {
    serde_json::from_str::<UniqueObject>(input).map(|value| value.0)
}

fn insert_top_level_member_commas(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(input.len() + 8);
    let mut index = 0;
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            output.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => {
                in_string = true;
                output.push(b'"');
                index += 1;
            }
            b'{' | b'[' => {
                depth = depth.checked_add(1)?;
                output.push(byte);
                index += 1;
            }
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                output.push(byte);
                index += 1;
            }
            value if value.is_ascii_whitespace() && depth == 0 => {
                let start = index;
                while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                output.extend_from_slice(&bytes[start..index]);
                if index < bytes.len() && bytes[index] == b'"' {
                    let previous = output
                        .iter()
                        .rev()
                        .copied()
                        .find(|value| !value.is_ascii_whitespace())?;
                    if matches!(previous, b'}' | b']' | b'"' | b'0'..=b'9' | b'e' | b'l') {
                        output.push(b',');
                    }
                }
            }
            _ => {
                output.push(byte);
                index += 1;
            }
        }
    }
    if in_string || depth != 0 {
        None
    } else {
        String::from_utf8(output).ok()
    }
}

pub fn build_runtime(
    proposal: &Proposal,
    identity: &IdentityRecord,
) -> Result<(Value, Value), ConfigError> {
    validate_identity(identity)?;
    if identity.alias != proposal.flight_sheet.identity_ref
        && proposal.flight_sheet.identity_ref != "unresolved"
        && !proposal.flight_sheet.identity_ref.starts_with("hive-wal-")
    {
        return Err(error(
            "RIGOS_FLIGHT_SHEET_UNRESOLVED_IDENTITY",
            None,
            None,
            Some("identity_ref"),
            "selected identity alias does not match the flight sheet",
        ));
    }
    let sheet = &proposal.flight_sheet;
    let worker = sheet
        .worker_template
        .replace("{node_name}", &proposal.profile.node_name);
    let policy = json!({"schema":"rigos.policy/v1","node_name":proposal.profile.node_name,"timezone":proposal.profile.timezone,"active_flight_sheet":sheet.name,"identity_ref":identity.alias,"miner_start_mode":proposal.profile.miner_start_mode});
    let pools: Vec<_> = sheet.pools.iter().map(|pool| json!({"url":format!("{}:{}", pool.host, pool.port),"user":identity.value,"pass":worker,"tls":pool.tls,"keepalive":true,"algo":sheet.algorithm})).collect();
    let mut cpu = Map::from_iter([
        ("enabled".into(), Value::Bool(true)),
        ("huge-pages".into(), Value::Bool(sheet.cpu.huge_pages)),
    ]);
    cpu.insert("max-threads-hint".into(), json!(sheet.cpu.max_threads_hint));
    if let Threads::Count(count) = sheet.cpu.threads {
        cpu.insert("max-threads-hint".into(), json!(count));
    }
    let xmrig = json!({"autosave":false,"background":false,"cpu":cpu,"pools":pools,"api":{"worker-id":worker},"http":{"enabled":false}});
    Ok((policy, xmrig))
}

pub fn commit_revision(
    state: &Path,
    proposal: &Proposal,
    identity: &IdentityRecord,
) -> Result<String, ConfigError> {
    let (policy, xmrig) = build_runtime(proposal, identity)?;
    let revisions = state.join("revisions");
    fs::create_dir_all(&revisions).map_err(io_error)?;
    let id = Uuid::new_v4().to_string();
    let staging = revisions.join(format!(".{id}.staging"));
    let final_dir = revisions.join(&id);
    fs::create_dir(&staging).map_err(io_error)?;
    let result = (|| {
        write_json(&staging.join("policy.json"), &policy, 0o640)?;
        write_json(&staging.join("xmrig.json"), &xmrig, 0o640)?;
        fs::create_dir(staging.join("flight-sheets")).map_err(io_error)?;
        write_json(
            &staging
                .join("flight-sheets")
                .join(format!("{}.json", proposal.flight_sheet.name)),
            &proposal.flight_sheet,
            0o640,
        )?;
        fs::create_dir(staging.join("identities")).map_err(io_error)?;
        let prior_identities = state.join("current/identities");
        if prior_identities.is_dir() {
            for entry in fs::read_dir(&prior_identities).map_err(io_error)? {
                let entry = entry.map_err(io_error)?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
                if metadata.file_type().is_file()
                    && !metadata.file_type().is_symlink()
                    && safe_json_basename(&name)
                    && name != format!("{}.json", identity.alias)
                {
                    fs::copy(entry.path(), staging.join("identities").join(name))
                        .map_err(io_error)?;
                }
            }
        }
        write_json(
            &staging
                .join("identities")
                .join(format!("{}.json", identity.alias)),
            identity,
            0o600,
        )?;
        if let Some(provenance) = &proposal.provenance {
            write_json(&staging.join("import-provenance.json"), provenance, 0o640)?;
        }
        if let Some(reference) = proposal
            .provenance
            .as_ref()
            .and_then(|value| value.external_reference.as_ref())
        {
            write_json(
                &staging.join("external-identity-map.json"),
                &json!({
                    "schema":"rigos.external-identity-map/v1",
                    "mappings":[{
                        "source":reference.source,
                        "external_type":reference.external_type,
                        "external_value":reference.external_value,
                        "identity_ref":identity.alias,
                        "confirmed_source_sha256":proposal.source_sha256,
                    }]
                }),
                0o600,
            )?;
        }
        let owned = Command::new("chown")
            .args(["-R", "root:rigos"])
            .arg(&staging)
            .status()
            .map_err(io_error)?;
        if !owned.success() {
            return Err(error(
                "RIGOS_CONFIG_INVALID_VALUE",
                None,
                None,
                None,
                "failed to restrict revision ownership",
            ));
        }
        File::open(&staging)
            .and_then(|f| f.sync_all())
            .map_err(io_error)?;
        fs::rename(&staging, &final_dir).map_err(io_error)?;
        if let Ok(previous_target) = fs::read_link(state.join("current")) {
            let previous_link = state.join(format!(".previous-{id}"));
            create_relative_symlink(previous_target, &previous_link)?;
            fs::rename(previous_link, state.join("previous")).map_err(io_error)?;
        }
        let link = state.join(format!(".current-{id}"));
        create_relative_symlink(Path::new("revisions").join(&id), &link)?;
        fs::rename(&link, state.join("current")).map_err(io_error)?;
        ensure_public_link(state, "policy.json", Path::new("current/policy.json"))?;
        ensure_public_link(state, "xmrig.json", Path::new("current/xmrig.json"))?;
        ensure_public_link(state, "flight-sheets", Path::new("current/flight-sheets"))?;
        ensure_public_link(state, "identities", Path::new("current/identities"))?;
        ensure_public_link(
            state,
            "external-identity-map.json",
            Path::new("current/external-identity-map.json"),
        )?;
        File::open(state)
            .and_then(|f| f.sync_all())
            .map_err(io_error)?;
        Ok(id.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn safe_json_basename(name: &str) -> bool {
    !name.starts_with('.')
        && name.ends_with(".json")
        && name.len() <= 128
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

pub fn safe_join(root: &Path, name: &str) -> Result<PathBuf, ConfigError> {
    if !safe_json_basename(name)
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(error(
            "RIGOS_CONFIG_INVALID_VALUE",
            None,
            None,
            Some("FLIGHT_REF"),
            "unsafe filename",
        ));
    }
    Ok(root.join(name))
}

pub fn validate_identity(identity: &IdentityRecord) -> Result<(), ConfigError> {
    if identity.schema != "rigos.identity/v1"
        || identity.kind != "mining_identity"
        || !valid_slug(&identity.alias, 64)
        || identity.value.is_empty()
        || identity.value.len() > 512
        || identity
            .value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(error(
            "RIGOS_FLIGHT_SHEET_UNRESOLVED_IDENTITY",
            None,
            None,
            Some("identity_ref"),
            "invalid local mining identity",
        ));
    }
    Ok(())
}

fn valid_slug(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn valid_timezone_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('/')
        && !value.contains("..")
        && value.split('/').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+'))
        })
}
fn valid_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && (value.contains(':')
            || value.split('.').all(|p| {
                !p.is_empty()
                    && p.len() <= 63
                    && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }))
}
fn string_field<'a>(object: &'a Map<String, Value>, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(Value::as_str))
}
fn value_as_reference(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
fn string_list(value: Option<&Value>) -> Option<Vec<String>> {
    match value? {
        Value::Array(v) => Some(
            v.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        ),
        Value::String(v) => Some(vec![v.clone()]),
        _ => None,
    }
}
fn bool_list(value: Option<&Value>, count: usize) -> Vec<bool> {
    match value {
        Some(Value::Array(v)) => v.iter().map(|x| x.as_bool().unwrap_or(false)).collect(),
        Some(Value::Bool(v)) => vec![*v; count],
        _ => vec![false; count],
    }
}
fn lifecycle_warnings(object: &Map<String, Value>) -> Vec<String> {
    ["autostart", "enabled", "start_on_boot"]
        .into_iter()
        .filter(|key| object.contains_key(*key))
        .map(|key| format!("lifecycle field {key} was not imported"))
        .collect()
}
fn io_error(error_value: std::io::Error) -> ConfigError {
    error(
        "RIGOS_CONFIG_INVALID_VALUE",
        None,
        None,
        None,
        format!("local state operation failed: {error_value}"),
    )
}
fn write_json<T: Serialize>(path: &Path, value: &T, _mode: u32) -> Result<(), ConfigError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(_mode);
    }
    let mut file = options.open(path).map_err(io_error)?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|e| {
        error(
            "RIGOS_CONFIG_INVALID_VALUE",
            None,
            None,
            None,
            format!("JSON write failed: {e}"),
        )
    })?;
    file.write_all(b"\n")
        .and_then(|_| file.sync_all())
        .map_err(io_error)
}
#[cfg(unix)]
fn create_relative_symlink(target: PathBuf, link: &Path) -> Result<(), ConfigError> {
    std::os::unix::fs::symlink(target, link).map_err(io_error)
}
#[cfg(not(unix))]
fn create_relative_symlink(_target: PathBuf, _link: &Path) -> Result<(), ConfigError> {
    Err(error(
        "RIGOS_CONFIG_INVALID_VALUE",
        None,
        None,
        None,
        "atomic revisions require Unix symlinks",
    ))
}
fn ensure_public_link(state: &Path, name: &str, target: &Path) -> Result<(), ConfigError> {
    let link = state.join(name);
    if !link.exists() && fs::symlink_metadata(&link).is_err() {
        create_relative_symlink(target.to_path_buf(), &link)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile(source: &str, reference: &str) -> Vec<u8> {
        format!("RIGOS_CONFIG_VERSION=1\nNODE_NAME=rig01\nTIMEZONE=Asia/Bangkok\nFLIGHT_SOURCE={source}\n{reference}MINER_START_MODE=manual\n").into_bytes()
    }
    #[test]
    fn parses_strict_profile() {
        assert_eq!(
            parse_rig_profile(&profile("native", "FLIGHT_REF=xmr-ssl\n"))
                .unwrap()
                .node_name,
            "rig01"
        );
    }
    #[test]
    fn duplicate_key_reports_line() {
        let e = parse_rig_profile(
            &[
                profile("native", "FLIGHT_REF=xmr-ssl\n"),
                b"NODE_NAME=x\n".to_vec(),
            ]
            .concat(),
        )
        .unwrap_err();
        assert_eq!(e.diagnostic.code, "RIGOS_CONFIG_DUPLICATE_KEY");
        assert_eq!(e.diagnostic.line, Some(7));
    }
    #[test]
    fn rejects_reserved_and_shell_content() {
        assert_eq!(
            parse_rig_profile(
                &[
                    profile("native", "FLIGHT_REF=xmr-ssl\n"),
                    b"WATCHDOG_ENABLED=0\n".to_vec()
                ]
                .concat()
            )
            .unwrap_err()
            .diagnostic
            .code,
            "RIGOS_CONFIG_UNSUPPORTED_KEY"
        );
        assert_eq!(
            parse_rig_profile(&profile("native", "FLIGHT_REF=$(id)\n"))
                .unwrap_err()
                .diagnostic
                .code,
            "RIGOS_CONFIG_SYNTAX"
        );
    }
    #[test]
    fn selection_matrix_is_strict() {
        assert!(parse_rig_profile(&profile("interactive", "")).is_ok());
        assert!(parse_rig_profile(&profile("interactive", "FLIGHT_REF=x\n")).is_err());
        assert!(parse_rig_profile(&profile("import", "")).is_err());
    }
    #[test]
    fn filenames_are_basenames() {
        assert!(safe_json_basename("sheet-1.json"));
        assert!(!safe_json_basename("../sheet.json"));
        assert!(!safe_json_basename("dir/sheet.json"));
    }
    #[test]
    fn hive_wallet_id_is_never_runtime_identity() {
        let raw = br#"{"name":"XMR","miner":"xmrig","pool_urls":["pool.example:443"],"pool_ssl":true,"wal_id":"fixture-wallet-ref"}"#;
        let (sheet, provenance) = import_hive_style(raw, "fixture.json").unwrap();
        assert_eq!(sheet.identity_ref, "hive-wal-fixture-wallet-ref");
        assert_eq!(
            provenance.external_reference.unwrap().external_value,
            "fixture-wallet-ref"
        );
    }
    #[test]
    fn identity_is_redacted_from_policy() {
        let p = Proposal {
            schema: "rigos.config-proposal/v1".into(),
            profile: parse_rig_profile(&profile("native", "FLIGHT_REF=xmr-ssl\n")).unwrap(),
            flight_sheet: FlightSheet {
                schema: "rigos.flight-sheet/v1".into(),
                name: "xmr-ssl".into(),
                coin: "XMR".into(),
                backend: "xmrig".into(),
                algorithm: "rx/0".into(),
                pools: vec![Pool {
                    host: "pool.example".into(),
                    port: 443,
                    tls: true,
                    priority: 0,
                }],
                identity_ref: "main-xmr".into(),
                worker_template: "{node_name}".into(),
                cpu: CpuPolicy {
                    threads: Threads::Auto("auto".into()),
                    huge_pages: true,
                    max_threads_hint: 100,
                },
            },
            provenance: None,
            source_sha256: "x".into(),
        };
        let i = IdentityRecord {
            schema: "rigos.identity/v1".into(),
            alias: "main-xmr".into(),
            kind: "mining_identity".into(),
            value: "SECRET".into(),
            created_locally: true,
        };
        let (policy, xmrig) = build_runtime(&p, &i).unwrap();
        assert!(!policy.to_string().contains("SECRET"));
        assert!(xmrig.to_string().contains("SECRET"));
    }

    #[test]
    fn flight_sheet_rejects_lifecycle_and_duplicate_pool_priority() {
        let lifecycle = br#"{"schema":"rigos.flight-sheet/v1","name":"xmr","coin":"XMR","backend":"xmrig","algorithm":"rx/0","pools":[{"host":"pool.example","port":443,"tls":true,"priority":0}],"identity_ref":"main-xmr","worker_template":"{node_name}","cpu":{"threads":"auto","huge_pages":true,"max_threads_hint":100},"autostart":true}"#;
        assert_eq!(
            parse_flight_sheet(lifecycle, "x.json")
                .unwrap_err()
                .diagnostic
                .code,
            "RIGOS_FLIGHT_SHEET_INVALID"
        );
        let duplicate = lifecycle
            .windows(lifecycle.len() - 18)
            .next()
            .unwrap_or(lifecycle);
        assert!(parse_flight_sheet(duplicate, "x.json").is_err());
    }

    #[test]
    fn huge_pages_false_reaches_runtime_config() {
        let mut proposal = Proposal {
            schema: "rigos.config-proposal/v1".into(),
            profile: parse_rig_profile(&profile("native", "FLIGHT_REF=xmr-ssl\n")).unwrap(),
            flight_sheet: FlightSheet {
                schema: "rigos.flight-sheet/v1".into(),
                name: "xmr-ssl".into(),
                coin: "XMR".into(),
                backend: "xmrig".into(),
                algorithm: "rx/0".into(),
                pools: vec![Pool {
                    host: "pool.example".into(),
                    port: 443,
                    tls: true,
                    priority: 0,
                }],
                identity_ref: "main-xmr".into(),
                worker_template: "{node_name}".into(),
                cpu: CpuPolicy {
                    threads: Threads::Auto("auto".into()),
                    huge_pages: false,
                    max_threads_hint: 100,
                },
            },
            provenance: None,
            source_sha256: "x".into(),
        };
        proposal.flight_sheet.cpu.huge_pages = false;
        let identity = IdentityRecord {
            schema: "rigos.identity/v1".into(),
            alias: "main-xmr".into(),
            kind: "mining_identity".into(),
            value: "private-value".into(),
            created_locally: true,
        };
        let (_, xmrig) = build_runtime(&proposal, &identity).unwrap();
        assert_eq!(xmrig["cpu"]["huge-pages"], false);
    }

    #[test]
    fn importer_rejects_cloud_credentials_and_reports_lifecycle() {
        let secret =
            br#"{"miner":"xmrig","pool_urls":["pool.example:443"],"rig_passwd":"do-not-copy"}"#;
        assert!(import_hive_style(secret, "fixture.json").is_err());
        let lifecycle = br#"{"miner":"xmrig","pool_urls":["pool.example:443"],"autostart":true}"#;
        let (_, provenance) = import_hive_style(lifecycle, "fixture.json").unwrap();
        assert_eq!(provenance.warnings.len(), 1);
        assert!(!serde_json::to_string(&provenance).unwrap().contains("true"));
    }

    #[test]
    fn strict_member_fragments_accept_real_shapes() {
        let cpu = parse_json_member_fragment(
            "\"huge-pages\": true,\n\"max-threads-hint\": 75",
            "fixture.json",
            "cpu_config",
        )
        .unwrap();
        assert_eq!(cpu["huge-pages"], true);
        let user = parse_json_member_fragment(
            "\"cpu\": {\"note\": \"brace } ไทย and newline\\n stay data\"}\n\"api\": {\"worker-id\": \"fixture\"}",
            "fixture.json",
            "user_config",
        )
        .unwrap();
        assert!(user["cpu"].is_object());
        assert!(user["api"].is_object());
    }

    #[test]
    fn strict_member_fragments_reject_duplicates_and_trailing_data() {
        for input in [
            "\"cpu\": {}\n\"cpu\": {}",
            "{\"cpu\":{}} trailing",
            "\"cpu\": {} garbage",
            "\"cpu\": {",
        ] {
            assert!(parse_json_member_fragment(input, "fixture.json", "user_config").is_err());
        }
    }
}
