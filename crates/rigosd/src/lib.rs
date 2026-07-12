#![forbid(unsafe_code)]

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use rigos_core::{CliEnvelope, Diagnostic, ExecutionStatus};
use rigos_machine::{MACHINE_SCHEMA, MachineContext};
use rigos_miner::MinerBackend;
use rigos_schema::{
    ABOUT_SCHEMA, AboutReportV1, COMPONENT_PROVENANCE_SCHEMA, ComponentProvenanceV1, DOCTOR_SCHEMA,
    DoctorCheckV1, HugePageAuthorityStatusV1, LICENSES_SCHEMA, LicenseEntryV1, LicensesReportV1,
    PERFORMANCE_STATUS_SCHEMA, PerformanceStatusV1, ReleaseInfoV1, doctor,
};
use rigos_xmrig::{MINER_SCHEMA, XmrigBackend};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::OnceLock,
};

#[derive(Parser)]
#[command(version = version_text(), about = "RIGOS local read-only inspector")]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    xmrig_executable: Option<PathBuf>,
    #[arg(long, global = true, value_name = "PATH")]
    xmrig_config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Machine {
        #[command(subcommand)]
        command: InspectCommand,
    },
    Miner {
        #[command(subcommand)]
        command: MinerCommand,
    },
    Health {
        #[command(subcommand)]
        command: InspectCommand,
    },
    State {
        #[command(subcommand)]
        command: InspectCommand,
    },
    Network {
        #[command(subcommand)]
        command: InspectCommand,
    },
    Doctor(OutputArgs),
    About(OutputArgs),
    Licenses(OutputArgs),
}

#[derive(Subcommand)]
enum InspectCommand {
    Inspect(OutputArgs),
}

#[derive(Subcommand)]
enum MinerCommand {
    Inspect(OutputArgs),
    Provenance(OutputArgs),
}

#[derive(Args, Clone, Copy)]
struct OutputArgs {
    #[arg(long)]
    json: bool,
}

fn version_text() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        format!(
            "{}\nproduct: RIGOS\nimage: {}\nimage-version: {}\nchannel: {}\ncommit: {}\ntarget: {}\nprofile: {}",
            env!("CARGO_PKG_VERSION"),
            option_env!("RIGOS_IMAGE_ID").unwrap_or("not-an-image-build"),
            option_env!("RIGOS_IMAGE_VERSION").unwrap_or("not-an-image-build"),
            option_env!("RIGOS_IMAGE_CHANNEL").unwrap_or("not-an-image-build"),
            env!("RIGOS_BUILD_COMMIT"),
            env!("RIGOS_BUILD_TARGET"),
            env!("RIGOS_BUILD_PROFILE")
        )
    })
}

fn command_name() -> &'static str {
    match std::env::current_exe().ok().and_then(|v| {
        v.file_stem()
            .map(|v| v.to_string_lossy().to_ascii_lowercase())
    }) {
        Some(name) if name == "rigosctl" => "rigosctl",
        _ => "rigosd",
    }
}

pub fn run() -> ExitCode {
    let matches = Cli::command().name(command_name()).get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(value) => value,
        Err(error) => error.exit(),
    };
    execute(cli)
}

fn execute(cli: Cli) -> ExitCode {
    let ctx = MachineContext::default();
    let backend = XmrigBackend {
        explicit_executable: cli.xmrig_executable,
        explicit_config: cli.xmrig_config,
        probe_version: true,
    };
    match cli.command {
        Command::Machine {
            command: InspectCommand::Inspect(output),
        } => {
            let result = rigos_machine::inspect(&ctx);
            render(
                output.json,
                CliEnvelope::new(
                    "machine.inspect",
                    MACHINE_SCHEMA,
                    result.value,
                    result.diagnostics,
                    result.fatal,
                ),
            )
        }
        Command::Miner {
            command: MinerCommand::Inspect(output),
        } => {
            let result = backend.discover(&ctx);
            render(
                output.json,
                CliEnvelope::new(
                    "miner.inspect",
                    MINER_SCHEMA,
                    result.value,
                    result.diagnostics,
                    result.fatal,
                ),
            )
        }
        Command::Doctor(output) => {
            let machine = rigos_machine::inspect(&ctx);
            let miner = backend.discover(&ctx);
            let mut data = doctor(&machine.diagnostics, &miner.diagnostics);
            data.checks.push(load_huge_page_check());
            data.checks.push(load_status_check(
                "state.ready",
                state_status_path(),
                "rigos.state-status/v1",
                "outcome",
                &["ready"],
            ));
            data.checks.push(load_status_check(
                "runtime.activation",
                activation_status_path(),
                "rigos.activation-status/v1",
                "outcome",
                &["ready"],
            ));
            data.checks.push(load_status_check(
                "miner.health",
                miner_health_status_path(),
                "rigos.miner-health-status/v1",
                "state",
                &["ready", "warming_up", "waiting_external"],
            ));
            data.checks.push(network_check());
            data.checks.push(log_bounds_check());
            data.checks.sort_by(|left, right| left.id.cmp(&right.id));
            let diagnostics: Vec<Diagnostic> = machine
                .diagnostics
                .into_iter()
                .chain(miner.diagnostics)
                .collect();
            render(
                output.json,
                CliEnvelope::new("doctor", DOCTOR_SCHEMA, Some(data), diagnostics, false),
            )
        }
        Command::About(output) => match load_about() {
            Ok(value) => render_about(output.json, value),
            Err(message) => render::<AboutReportV1>(
                output.json,
                CliEnvelope::new(
                    "about",
                    ABOUT_SCHEMA,
                    None,
                    vec![Diagnostic::error("about.unavailable", "identity", message)],
                    true,
                ),
            ),
        },
        Command::Licenses(output) => {
            let report = LicensesReportV1 {
                entries: vec![LicenseEntryV1 {
                    component: "xmrig".into(),
                    license: "GPL-3.0-or-later".into(),
                    notice_path: "/usr/share/rigos/THIRD_PARTY_NOTICES".into(),
                    license_path: "/usr/share/rigos/licenses/XMRig-GPL-3.0.txt".into(),
                }],
            };
            render_licenses(output.json, report)
        }
        Command::Miner {
            command: MinerCommand::Provenance(output),
        } => match load_provenance() {
            Ok(value) => render_provenance(output.json, value),
            Err(message) => render::<ComponentProvenanceV1>(
                output.json,
                CliEnvelope::new(
                    "miner.provenance",
                    COMPONENT_PROVENANCE_SCHEMA,
                    None,
                    vec![Diagnostic::error(
                        "miner.provenance_unavailable",
                        "miner",
                        message,
                    )],
                    true,
                ),
            ),
        },
        Command::Health {
            command: InspectCommand::Inspect(output),
        } => render_json_status(
            output.json,
            "health.inspect",
            "rigos.health-inspect/v1",
            load_health_report(),
        ),
        Command::State {
            command: InspectCommand::Inspect(output),
        } => render_json_status(
            output.json,
            "state.inspect",
            "rigos.state-inspect/v1",
            load_state_report(),
        ),
        Command::Network {
            command: InspectCommand::Inspect(output),
        } => render_json_status(
            output.json,
            "network.inspect",
            "rigos.network-inspect/v1",
            load_network_report(),
        ),
    }
}

fn performance_status_path() -> PathBuf {
    std::env::var_os("RIGOS_PERFORMANCE_STATUS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/rigos/performance-status.json"))
}

fn boot_id_path() -> PathBuf {
    std::env::var_os("RIGOS_BOOT_ID_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/proc/sys/kernel/random/boot_id"))
}

fn current_revision_path() -> PathBuf {
    std::env::var_os("RIGOS_CURRENT_REVISION_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/rigos/current"))
}

fn runtime_path() -> PathBuf {
    std::env::var_os("RIGOS_RUNTIME_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/rigos"))
}

fn state_root_path() -> PathBuf {
    std::env::var_os("RIGOS_STATE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/rigos"))
}

fn state_status_path() -> PathBuf {
    runtime_path().join("state-status.json")
}

fn activation_status_path() -> PathBuf {
    state_root_path().join("activation-status.json")
}

fn runtime_config_status_path() -> PathBuf {
    runtime_path().join("runtime-config-status.json")
}

fn miner_health_status_path() -> PathBuf {
    runtime_path().join("miner-health-status.json")
}

fn proc_net_route_path() -> PathBuf {
    std::env::var_os("RIGOS_PROC_NET_ROUTE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/proc/net/route"))
}

fn resolv_conf_path() -> PathBuf {
    std::env::var_os("RIGOS_RESOLV_CONF")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/resolv.conf"))
}

fn journald_config_path() -> PathBuf {
    std::env::var_os("RIGOS_JOURNALD_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/systemd/journald.conf.d/rigos.conf"))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&content).map_err(|error| error.to_string())
}

fn read_json_or_error(path: &Path) -> Value {
    match read_json(path) {
        Ok(value) => value,
        Err(error) => json!({"error": error}),
    }
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn load_status_check(
    id: &str,
    path: PathBuf,
    schema: &str,
    field: &str,
    pass_values: &[&str],
) -> DoctorCheckV1 {
    let value = match read_json(&path) {
        Ok(value) => value,
        Err(error) => {
            return DoctorCheckV1 {
                id: id.into(),
                status: "fail".into(),
                summary: format!("status unavailable: {error}"),
            };
        }
    };
    if json_string(&value, "schema") != Some(schema) {
        return DoctorCheckV1 {
            id: id.into(),
            status: "fail".into(),
            summary: "schema mismatch".into(),
        };
    }
    let observed = json_string(&value, field).unwrap_or("unknown");
    DoctorCheckV1 {
        id: id.into(),
        status: if pass_values.contains(&observed) {
            "pass"
        } else {
            "warning"
        }
        .into(),
        summary: format!("{field}={observed}"),
    }
}

fn default_route_present(route: &str) -> bool {
    route.lines().skip(1).any(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        fields.get(1) == Some(&"00000000")
    })
}

fn dns_configured(resolv: &str) -> bool {
    resolv
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with("nameserver "))
}

fn load_network_report() -> Result<Value, String> {
    let route = fs::read_to_string(proc_net_route_path()).unwrap_or_default();
    let resolv = fs::read_to_string(resolv_conf_path()).unwrap_or_default();
    let default_route = default_route_present(&route);
    let dns = dns_configured(&resolv);
    Ok(json!({
        "schema": "rigos.network-inspect/v1",
        "default_route": default_route,
        "dns_configured": dns,
        "state": if default_route && dns { "ready" } else { "degraded" },
        "reason": if default_route && dns { Value::Null } else { json!("network_or_dns_unavailable") }
    }))
}

fn network_check() -> DoctorCheckV1 {
    match load_network_report() {
        Ok(value) => {
            let state = json_string(&value, "state").unwrap_or("unknown");
            DoctorCheckV1 {
                id: "network.inspect".into(),
                status: if state == "ready" { "pass" } else { "warning" }.into(),
                summary: format!("state={state}"),
            }
        }
        Err(error) => DoctorCheckV1 {
            id: "network.inspect".into(),
            status: "warning".into(),
            summary: error,
        },
    }
}

fn log_bounds_check() -> DoctorCheckV1 {
    let content = match fs::read_to_string(journald_config_path()) {
        Ok(value) => value,
        Err(error) => {
            return DoctorCheckV1 {
                id: "logs.bounds".into(),
                status: "fail".into(),
                summary: format!("journald bounds unavailable: {error}"),
            };
        }
    };
    let bounded = content.contains("RuntimeMaxUse=32M") && content.contains("Storage=volatile");
    DoctorCheckV1 {
        id: "logs.bounds".into(),
        status: if bounded { "pass" } else { "fail" }.into(),
        summary: if bounded {
            "volatile journald capped at 32M".into()
        } else {
            "journald bounds missing".into()
        },
    }
}

fn load_health_report() -> Result<Value, String> {
    Ok(json!({
        "schema": "rigos.health-inspect/v1",
        "miner": read_json_or_error(&miner_health_status_path()),
        "network": load_network_report().unwrap_or_else(|error| json!({"error": error})),
        "state": read_json_or_error(&state_status_path()),
        "runtime": read_json_or_error(&runtime_config_status_path()),
        "activation": read_json_or_error(&activation_status_path()),
    }))
}

fn load_state_report() -> Result<Value, String> {
    let current = fs::read_link(current_revision_path())
        .ok()
        .and_then(|value| {
            value
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
        });
    Ok(json!({
        "schema": "rigos.state-inspect/v1",
        "current_revision": current,
        "state_status": read_json_or_error(&state_status_path()),
        "activation_status": read_json_or_error(&activation_status_path()),
        "runtime_config_status": read_json_or_error(&runtime_config_status_path()),
    }))
}

fn load_huge_page_check() -> DoctorCheckV1 {
    let status = match fs::read(performance_status_path()) {
        Ok(value) => value,
        Err(error) => {
            return failed_huge_page_check(format!("performance status unavailable: {error}"));
        }
    };
    let boot_id = match fs::read_to_string(boot_id_path()) {
        Ok(value) if !value.trim().is_empty() => value.trim().to_owned(),
        Ok(_) => return failed_huge_page_check("boot ID is empty".into()),
        Err(error) => return failed_huge_page_check(format!("boot ID unavailable: {error}")),
    };
    let revision = match fs::read_link(current_revision_path()) {
        Ok(value) => match value.file_name().and_then(|value| value.to_str()) {
            Some(value) if !value.is_empty() => Some(value.to_owned()),
            _ => return failed_huge_page_check("current revision target is invalid".into()),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return failed_huge_page_check(format!("current revision unavailable: {error}"));
        }
    };
    evaluate_huge_page_check(&status, &boot_id, revision.as_deref())
}

fn evaluate_huge_page_check(status: &[u8], boot_id: &str, revision: Option<&str>) -> DoctorCheckV1 {
    let status: PerformanceStatusV1 = match serde_json::from_slice(status) {
        Ok(value) => value,
        Err(error) => return failed_huge_page_check(format!("invalid status JSON: {error}")),
    };
    if status.schema != PERFORMANCE_STATUS_SCHEMA {
        return failed_huge_page_check("performance status schema mismatch".into());
    }
    if status.boot_id != boot_id {
        return failed_huge_page_check("performance status is from another boot".into());
    }
    if status.config_revision.as_deref() != revision {
        return failed_huge_page_check("performance status uses another config revision".into());
    }
    let level = match status.huge_pages.status {
        HugePageAuthorityStatusV1::Ready | HugePageAuthorityStatusV1::Disabled => "pass",
        _ => "warning",
    };
    DoctorCheckV1 {
        id: "performance.huge_pages".into(),
        status: level.into(),
        summary: format!(
            "{} {} of {} pages",
            huge_page_status_name(&status.huge_pages.status),
            status.huge_pages.actual_pages,
            status.huge_pages.target_pages
        ),
    }
}

fn huge_page_status_name(status: &HugePageAuthorityStatusV1) -> &'static str {
    match status {
        HugePageAuthorityStatusV1::NotProvisioned => "not_provisioned",
        HugePageAuthorityStatusV1::Ready => "ready",
        HugePageAuthorityStatusV1::Disabled => "disabled",
        HugePageAuthorityStatusV1::DegradedInsufficientMemory => "degraded_insufficient_memory",
        HugePageAuthorityStatusV1::DegradedPartialAllocation => "degraded_partial_allocation",
        HugePageAuthorityStatusV1::DegradedUnavailable => "degraded_unavailable",
        HugePageAuthorityStatusV1::DegradedUnsupported => "degraded_unsupported",
        HugePageAuthorityStatusV1::DegradedReleaseIncomplete => "degraded_release_incomplete",
    }
}

fn failed_huge_page_check(summary: String) -> DoctorCheckV1 {
    DoctorCheckV1 {
        id: "performance.huge_pages".into(),
        status: "fail".into(),
        summary,
    }
}

fn release_path() -> PathBuf {
    std::env::var_os("RIGOS_RELEASE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/rigos-release"))
}

fn provenance_path() -> PathBuf {
    std::env::var_os("RIGOS_XMRIG_PROVENANCE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/share/rigos/components/xmrig.json"))
}

fn parse_release() -> Result<ReleaseInfoV1, String> {
    let content = fs::read_to_string(release_path()).map_err(|error| error.to_string())?;
    let mut fields = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid release field: {line}"))?;
        let value = value.trim();
        let decoded = if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            value[1..value.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\\\", "\\")
        } else {
            value.to_owned()
        };
        fields.insert(key.to_owned(), decoded);
    }
    let required = |name: &str| {
        fields
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing release field: {name}"))
    };
    Ok(ReleaseInfoV1 {
        schema: required("RIGOS_SCHEMA")?,
        product: required("NAME")?,
        product_version: required("VERSION_ID")?,
        image_id: required("IMAGE_ID")?,
        image_version: required("IMAGE_VERSION")?,
        image_channel: required("IMAGE_CHANNEL")?,
        variant: required("VARIANT")?,
        architecture: required("ARCHITECTURE")?,
        base_id: required("BASE_ID")?,
        base_version_id: required("BASE_VERSION_ID")?,
        build_id: required("BUILD_ID")?,
        build_commit: required("BUILD_COMMIT")?,
    })
}

fn load_provenance() -> Result<ComponentProvenanceV1, String> {
    let content = fs::read_to_string(provenance_path()).map_err(|error| error.to_string())?;
    let value: ComponentProvenanceV1 =
        serde_json::from_str(&content).map_err(|error| error.to_string())?;
    validate_provenance(value)
}

fn validate_provenance(value: ComponentProvenanceV1) -> Result<ComponentProvenanceV1, String> {
    if value.schema != COMPONENT_PROVENANCE_SCHEMA
        || value.component != "xmrig"
        || value.modified
        || value.rigos_fee_percent != 0
        || value.rigos_receives_donation
        || value.upstream_donation_behavior != "applies"
    {
        return Err("component provenance violates the RIGOS miner contract".into());
    }
    Ok(value)
}

fn load_about() -> Result<AboutReportV1, String> {
    Ok(AboutReportV1 {
        release: parse_release()?,
        subscription: "none".into(),
        worker_limit: "none".into(),
        mining_fee_percent: 0,
        cloud_dependency: "none".into(),
        bundled_miner: load_provenance()?,
    })
}

fn render_about(json: bool, value: AboutReportV1) -> ExitCode {
    if json {
        return render(
            true,
            CliEnvelope::new("about", ABOUT_SCHEMA, Some(value), vec![], false),
        );
    }
    println!("RIGOS {}", value.release.product_version);
    println!("CPU-ONLY USB MINING OPERATING SYSTEM\n");
    println!("RIGOS subscription:       {}", value.subscription);
    println!("RIGOS worker limit:       {}", value.worker_limit);
    println!("RIGOS mining fee:         {}%", value.mining_fee_percent);
    println!("RIGOS cloud dependency:   {}\n", value.cloud_dependency);
    println!("Bundled miner:");
    println!("  XMRig {}", value.bundled_miner.version);
    println!("  Source: official upstream release");
    println!("  Modified by RIGOS: no");
    println!("  Upstream donation behavior: applies");
    println!("  Donation received by RIGOS: no");
    ExitCode::SUCCESS
}

fn render_provenance(json: bool, value: ComponentProvenanceV1) -> ExitCode {
    if json {
        return render(
            true,
            CliEnvelope::new(
                "miner.provenance",
                COMPONENT_PROVENANCE_SCHEMA,
                Some(value),
                vec![],
                false,
            ),
        );
    }
    println!("backend: xmrig");
    println!("version: {}", value.version);
    println!("distribution: official_upstream");
    println!("modified: false");
    println!("upstream_donation_behavior: applies");
    println!("rigos_fee_percent: 0");
    ExitCode::SUCCESS
}

fn render_licenses(json: bool, value: LicensesReportV1) -> ExitCode {
    if json {
        return render(
            true,
            CliEnvelope::new("licenses", LICENSES_SCHEMA, Some(value), vec![], false),
        );
    }
    for entry in value.entries {
        println!("{}: {}", entry.component, entry.license);
        println!("  notice: {}", entry.notice_path);
        println!("  license: {}", entry.license_path);
    }
    ExitCode::SUCCESS
}

fn render_json_status(
    json: bool,
    command: &str,
    schema: &str,
    value: Result<Value, String>,
) -> ExitCode {
    match value {
        Ok(value) => render(
            json,
            CliEnvelope::new(command, schema, Some(value), vec![], false),
        ),
        Err(message) => render::<Value>(
            json,
            CliEnvelope::new(
                command,
                schema,
                None,
                vec![Diagnostic::error("status.unavailable", "status", message)],
                true,
            ),
        ),
    }
}

fn render<T: Serialize + std::fmt::Debug>(json: bool, envelope: CliEnvelope<T>) -> ExitCode {
    let status = envelope.status.clone();
    if json {
        match serde_json::to_string_pretty(&envelope) {
            Ok(value) => println!("{value}"),
            Err(_) => return ExitCode::from(4),
        }
    } else {
        println!("{}: {:?}", envelope.command, envelope.status);
        println!("observed at: {}", envelope.observed_at);
        println!("{:#?}", envelope.data);
        for diagnostic in &envelope.diagnostics {
            println!(
                "[{:?}] {}: {}",
                diagnostic.severity, diagnostic.code, diagnostic.message
            );
        }
    }
    match status {
        ExecutionStatus::Error => ExitCode::from(3),
        _ => ExitCode::SUCCESS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_cli_alias_is_supported() {
        assert_eq!(Cli::command().name("rigosctl").get_name(), "rigosctl");
        assert_eq!(Cli::command().name("rigosd").get_name(), "rigosd");
    }

    #[test]
    fn alpha23_headless_observability_commands_are_registered() {
        let help = Cli::command().render_long_help().to_string();
        for command in ["health", "state", "network", "doctor", "miner", "machine"] {
            assert!(help.contains(command), "missing CLI command: {command}");
        }
    }

    #[test]
    fn network_inspection_distinguishes_route_and_dns_truth() {
        assert!(default_route_present(
            "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\neth0 00000000 0100000A 0003 0 0 100 00000000 0 0 0\n"
        ));
        assert!(!default_route_present(
            "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\neth0 0010A8C0 00000000 0001 0 0 100 00FFFFFF 0 0 0\n"
        ));
        assert!(dns_configured("nameserver 1.1.1.1\n"));
        assert!(!dns_configured("# nameserver intentionally absent\n"));
    }

    #[test]
    fn provenance_contract_rejects_false_fee_claims() {
        let mut value = ComponentProvenanceV1 {
            schema: COMPONENT_PROVENANCE_SCHEMA.into(),
            component: "xmrig".into(),
            version: "6.26.0".into(),
            source: "official-upstream-release".into(),
            modified: false,
            architecture: "x86_64".into(),
            artifact: "xmrig.tar.gz".into(),
            archive_sha256: "a".repeat(64),
            binary_sha256: "b".repeat(64),
            license: "GPL-3.0-or-later".into(),
            upstream_donation_behavior: "applies".into(),
            rigos_receives_donation: false,
            rigos_fee_percent: 0,
        };
        assert!(validate_provenance(value.clone()).is_ok());
        value.rigos_receives_donation = true;
        assert!(validate_provenance(value).is_err());
    }

    #[test]
    fn doctor_exposes_ready_degraded_and_stale_huge_page_truth() {
        let status = |kind| PerformanceStatusV1 {
            schema: PERFORMANCE_STATUS_SCHEMA.into(),
            boot_id: "boot-a".into(),
            generated_at: "2026-07-06T00:00:00.000Z".into(),
            config_revision: Some("revision-a".into()),
            algorithm: Some("rx/0".into()),
            huge_pages: rigos_schema::HugePageAuthorityV1 {
                requested: true,
                target_pages: 1280,
                attempted_pages: 1280,
                actual_pages: 1280,
                huge_page_size_bytes: 2 * 1024 * 1024,
                memory_available_before_bytes: 4 * 1024 * 1024 * 1024,
                reserve_bytes: 1024 * 1024 * 1024,
                allocation_percent_of_target: 100.0,
                status: kind,
                reason: None,
            },
        };
        let ready = serde_json::to_vec(&status(HugePageAuthorityStatusV1::Ready)).unwrap();
        assert_eq!(
            evaluate_huge_page_check(&ready, "boot-a", Some("revision-a")).status,
            "pass"
        );
        for kind in [
            HugePageAuthorityStatusV1::DegradedInsufficientMemory,
            HugePageAuthorityStatusV1::DegradedPartialAllocation,
            HugePageAuthorityStatusV1::DegradedUnavailable,
            HugePageAuthorityStatusV1::DegradedUnsupported,
            HugePageAuthorityStatusV1::DegradedReleaseIncomplete,
        ] {
            let degraded = serde_json::to_vec(&status(kind)).unwrap();
            assert_eq!(
                evaluate_huge_page_check(&degraded, "boot-a", Some("revision-a")).status,
                "warning"
            );
        }
        let disabled = serde_json::to_vec(&status(HugePageAuthorityStatusV1::Disabled)).unwrap();
        assert_eq!(
            evaluate_huge_page_check(&disabled, "boot-a", Some("revision-a")).status,
            "pass"
        );
        assert_eq!(
            evaluate_huge_page_check(&ready, "boot-b", Some("revision-a")).status,
            "fail"
        );
    }
}
