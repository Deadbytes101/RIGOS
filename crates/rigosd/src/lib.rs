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
use std::{collections::BTreeMap, fs, path::PathBuf, process::ExitCode, sync::OnceLock};

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
