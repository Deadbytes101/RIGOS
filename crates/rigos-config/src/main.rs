#![forbid(unsafe_code)]

#[path = "lib_entry.rs"]
mod compatibility;

use clap::{Parser, Subcommand};
use rigos_config::{
    ConfigDiagnostic, ConfigError, FlightSource, IdentityRecord, MinerStartMode, Proposal,
    commit_revision, parse_flight_sheet, parse_rig_profile, safe_join,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};
use uuid::Uuid;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Prepare {
        #[arg(long, default_value = "/run/rigos/boot-device.json")]
        attestation: PathBuf,
        #[arg(long, default_value = "/run/rigos/state-status.json")]
        status: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        flight_source: Option<String>,
        #[arg(long)]
        flight_ref: Option<String>,
    },
    Discover {
        #[arg(long, default_value = "/run/rigos/boot-device.json")]
        attestation: PathBuf,
        #[arg(long, default_value = "/run/rigos/state-status.json")]
        status: PathBuf,
    },
    Commit {
        #[arg(long, default_value = "/var/lib/rigos")]
        state: PathBuf,
    },
    Activate {
        #[arg(long, default_value = "/var/lib/rigos")]
        state: PathBuf,
        #[arg(long, default_value = "/proc/cmdline")]
        cmdline: PathBuf,
    },
    Current {
        #[arg(long, default_value = "/var/lib/rigos")]
        state: PathBuf,
    },
    NeedsActivation {
        #[arg(long, default_value = "/var/lib/rigos")]
        state: PathBuf,
    },
    Gate {
        #[arg(long, default_value = "/var/lib/rigos")]
        state: PathBuf,
        #[arg(long, default_value = "/proc/cmdline")]
        cmdline: PathBuf,
    },
    Timezone {
        #[arg(long, default_value = "/var/lib/rigos")]
        state: PathBuf,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Attestation {
    schema: String,
    boot_id: String,
    verification_outcome: String,
    disk: AttestedDisk,
    partition1: AttestedPartition,
    root: AttestedRoot,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct AttestedDisk {
    #[serde(rename = "path")]
    _path: String,
    major_minor: String,
    ptuuid: String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct AttestedPartition {
    path: String,
    major_minor: String,
    partuuid: String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct AttestedRoot {
    major_minor: String,
}
#[derive(Deserialize)]
struct Status {
    outcome: String,
}
#[derive(Deserialize)]
struct CommitRequest {
    proposal: Proposal,
    identity: IdentityRecord,
}
#[derive(Clone, Deserialize, Serialize)]
struct Policy {
    timezone: String,
    miner_start_mode: MinerStartMode,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match execute(cli) {
        Ok(value) => {
            println!("{}", serde_json::to_string(&value).unwrap());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", serde_json::to_string(&error.diagnostic).unwrap());
            ExitCode::from(2)
        }
    }
}

fn execute(cli: Cli) -> Result<Value, ConfigError> {
    match cli.command {
        Commands::Prepare {
            attestation,
            status,
            output,
            flight_source,
            flight_ref,
        } => {
            let proposal = prepare(
                &attestation,
                &status,
                flight_source.as_deref(),
                flight_ref.as_deref(),
            )?;
            write_private_json(&output, &proposal)?;
            Ok(json!({"outcome":"prepared"}))
        }
        Commands::Discover {
            attestation,
            status,
        } => with_verified_efi(&attestation, &status, discover),
        Commands::Commit { state } => {
            let request = read_commit_request()?;
            let (revision, created) = commit_once(&state, &request)?;
            Ok(json!({"outcome":"configuration_committed","revision":revision,"created":created}))
        }
        Commands::Activate { state, cmdline } => {
            let (revision, miner_started) = activate(&state, &cmdline, &mut SystemRuntime)?;
            Ok(json!({"outcome":"ready","revision":revision,"miner_started":miner_started}))
        }
        Commands::Current { state } => match current_revision(&state)? {
            Some(revision) => Ok(json!({"outcome":"configuration_committed","revision":revision})),
            None => Ok(json!({"outcome":"preflight_failed","reason":"not_provisioned"})),
        },
        Commands::NeedsActivation { state } => {
            if activation_ready(&state)? {
                std::process::exit(1);
            }
            Ok(json!({"outcome":"activation_required"}))
        }
        Commands::Gate { state, cmdline } => {
            let allowed = gate(&state, &cmdline)?;
            if !allowed {
                std::process::exit(1);
            }
            Ok(json!({"outcome":"allowed"}))
        }
        Commands::Timezone { state } => {
            let policy_path = state.join("current/policy.json");
            if !policy_path.is_file() {
                return Ok(json!({"outcome":"no_configuration"}));
            }
            let policy: Policy = read_json(&policy_path)?;
            apply_timezone(&policy.timezone)?;
            Ok(json!({"outcome":"timezone_applied"}))
        }
    }
}

fn read_commit_request() -> Result<CommitRequest, ConfigError> {
    let mut input = Vec::new();
    io::stdin()
        .take(2 * 1024 * 1024)
        .read_to_end(&mut input)
        .map_err(io_failure)?;
    serde_json::from_slice(&input)
        .map_err(|_| diagnostic("RIGOS_CONFIG_INVALID_VALUE", "commit request is invalid"))
}

fn prepare(
    attestation_path: &Path,
    status_path: &Path,
    source: Option<&str>,
    reference: Option<&str>,
) -> Result<Proposal, ConfigError> {
    with_verified_efi(attestation_path, status_path, |root| {
        prepare_from_root(root, source, reference)
    })
}

fn with_verified_efi<T>(
    attestation_path: &Path,
    status_path: &Path,
    operation: impl FnOnce(&Path) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let attestation: Attestation = read_json(attestation_path)?;
    let status: Status = read_json(status_path)?;
    if attestation.schema != "rigos.boot-device/v1"
        || attestation.verification_outcome != "verified"
        || !state_allows_config(&status.outcome)
    {
        return Err(diagnostic(
            "RIGOS_CONFIG_BOOT_DEVICE_UNPROVEN",
            "verified persistent boot USB is required",
        ));
    }
    let fresh = revalidate_attestation(&attestation)?;
    let stage = PathBuf::from(format!("/run/rigos/config-stage-{}", Uuid::new_v4()));
    fs::create_dir_all(&stage).map_err(io_failure)?;
    let mounted = Command::new("mount")
        .args(["-o", "ro,nodev,nosuid,noexec", &fresh.partition1.path])
        .arg(&stage)
        .status()
        .map_err(io_failure)?
        .success();
    if !mounted {
        let _ = fs::remove_dir(&stage);
        return Err(diagnostic(
            "RIGOS_CONFIG_BOOT_DEVICE_UNPROVEN",
            "exact EFI partition could not be mounted read only",
        ));
    }
    let mounted_identity = Command::new("findmnt")
        .args(["--noheadings", "--output", "MAJ:MIN", "--target"])
        .arg(&stage)
        .output()
        .map_err(io_failure)?;
    if !mounted_identity.status.success()
        || String::from_utf8_lossy(&mounted_identity.stdout).trim() != fresh.partition1.major_minor
    {
        let _ = Command::new("umount").arg(&stage).status();
        let _ = fs::remove_dir_all(&stage);
        return Err(diagnostic(
            "RIGOS_CONFIG_BOOT_DEVICE_UNPROVEN",
            "mounted EFI identity changed after verification",
        ));
    }
    let result = operation(&stage);
    let _ = Command::new("umount").arg(&stage).status();
    let _ = fs::remove_dir_all(&stage);
    result
}

fn revalidate_attestation(expected: &Attestation) -> Result<Attestation, ConfigError> {
    let current_boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map_err(io_failure)?
        .trim()
        .to_owned();
    if current_boot_id != expected.boot_id {
        return Err(diagnostic(
            "RIGOS_CONFIG_BOOT_DEVICE_UNPROVEN",
            "boot device attestation belongs to another boot",
        ));
    }
    let temporary = PathBuf::from(format!("/run/rigos/revalidate-{}", Uuid::new_v4()));
    fs::create_dir(&temporary).map_err(io_failure)?;
    let attestation_path = temporary.join("boot-device.json");
    let status_path = temporary.join("state-status.json");
    let result = Command::new("/usr/lib/rigos/rigos-state-init")
        .arg("--dry-run")
        .arg("--attestation-only")
        .arg("--attestation")
        .arg(&attestation_path)
        .arg("--status")
        .arg(&status_path)
        .status()
        .map_err(io_failure);
    let fresh = match result {
        Ok(status) if status.success() => read_json::<Attestation>(&attestation_path),
        _ => Err(diagnostic(
            "RIGOS_CONFIG_BOOT_DEVICE_UNPROVEN",
            "boot device revalidation failed",
        )),
    };
    let _ = fs::remove_dir_all(&temporary);
    let fresh = fresh?;
    if !attestation_identity_matches(expected, &fresh) {
        return Err(diagnostic(
            "RIGOS_CONFIG_BOOT_DEVICE_UNPROVEN",
            "boot device identity changed after attestation",
        ));
    }
    Ok(fresh)
}

fn attestation_identity_matches(expected: &Attestation, fresh: &Attestation) -> bool {
    expected.schema == "rigos.boot-device/v1"
        && fresh.schema == expected.schema
        && fresh.verification_outcome == "verified"
        && fresh.boot_id == expected.boot_id
        && fresh.disk.major_minor == expected.disk.major_minor
        && fresh
            .disk
            .ptuuid
            .eq_ignore_ascii_case(&expected.disk.ptuuid)
        && fresh.partition1.major_minor == expected.partition1.major_minor
        && fresh
            .partition1
            .partuuid
            .eq_ignore_ascii_case(&expected.partition1.partuuid)
        && fresh.root.major_minor == expected.root.major_minor
}

fn prepare_from_root(
    root: &Path,
    source_override: Option<&str>,
    reference_override: Option<&str>,
) -> Result<Proposal, ConfigError> {
    let config_path = root.join("rigos/rig.conf");
    let metadata = fs::symlink_metadata(&config_path)
        .map_err(|_| diagnostic("RIGOS_CONFIG_FILE_MISSING", "rig.conf is missing"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(diagnostic(
            "RIGOS_CONFIG_INVALID_VALUE",
            "rig.conf must be a regular file",
        ));
    }
    let config = read_bounded(&config_path, rigos_config::MAX_CONFIG_BYTES)?;
    let mut profile = parse_rig_profile(&config)?;
    if !Path::new("/usr/share/zoneinfo")
        .join(&profile.timezone)
        .is_file()
    {
        return Err(diagnostic(
            "RIGOS_CONFIG_INVALID_VALUE",
            "configured IANA timezone is not installed",
        ));
    }
    if let Some(source) = source_override {
        if profile.flight_source != FlightSource::Interactive {
            return Err(diagnostic(
                "RIGOS_CONFIG_INVALID_VALUE",
                "flight selection override requires interactive source",
            ));
        }
        profile.flight_source = match source {
            "native" => FlightSource::Native,
            "import" => FlightSource::Import,
            _ => {
                return Err(diagnostic(
                    "RIGOS_CONFIG_INVALID_VALUE",
                    "invalid interactive source selection",
                ));
            }
        };
        profile.flight_ref = reference_override.map(str::to_owned);
    }
    let (mut sheet, provenance, sheet_bytes) = match profile.flight_source {
        FlightSource::Native => {
            let filename = format!("{}.json", profile.flight_ref.as_deref().unwrap());
            let path = safe_join(&root.join("rigos/flight-sheets"), &filename)?;
            let bytes = read_regular_bounded(&path, rigos_config::MAX_SHEET_BYTES)?;
            (parse_flight_sheet(&bytes, &filename)?, None, bytes)
        }
        FlightSource::Import => {
            let filename = profile.flight_ref.as_deref().unwrap();
            let path = safe_join(&root.join("rigos/import"), filename)?;
            let bytes = read_regular_bounded(&path, rigos_config::MAX_SHEET_BYTES)?;
            let (sheet, provenance) = compatibility::import_hive_style(&bytes, filename)?;
            (sheet, Some(provenance), bytes)
        }
        FlightSource::Interactive => {
            return Err(diagnostic(
                "RIGOS_FLIGHT_SHEET_MISSING",
                "interactive flight selection is required",
            ));
        }
    };
    if let Some(provenance) = &provenance {
        if provenance.external_reference.is_some() {
            sheet.identity_ref = "unresolved".into();
        }
    }
    Ok(Proposal {
        schema: "rigos.config-proposal/v1".into(),
        profile,
        flight_sheet: sheet,
        provenance,
        source_sha256: hex::encode(Sha256::digest(&sheet_bytes)),
    })
}

fn discover(root: &Path) -> Result<Value, ConfigError> {
    fn names(path: PathBuf, native: bool) -> Result<Vec<String>, ConfigError> {
        let mut values = Vec::new();
        let mut folded = std::collections::BTreeSet::new();
        if !path.exists() {
            return Ok(values);
        }
        for entry in fs::read_dir(path).map_err(io_failure)?.take(128) {
            let entry = entry.map_err(io_failure)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_failure)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !metadata.file_type().is_file()
                || metadata.file_type().is_symlink()
                || !rigos_config::safe_json_basename(&name)
            {
                continue;
            }
            let folded_name = name.to_ascii_lowercase();
            if !folded.insert(folded_name) {
                return Err(diagnostic(
                    "RIGOS_FLIGHT_SHEET_INVALID",
                    "duplicate case insensitive filename",
                ));
            }
            values.push(if native {
                name.trim_end_matches(".json").to_owned()
            } else {
                name
            });
        }
        values.sort();
        Ok(values)
    }
    Ok(
        json!({"native":names(root.join("rigos/flight-sheets"), true)?,"import":names(root.join("rigos/import"), false)?}),
    )
}

trait RuntimeOps {
    fn set_timezone(&mut self, timezone: &str) -> Result<(), ConfigError>;
    fn set_miner_enabled(&mut self, enabled: bool) -> Result<(), ConfigError>;
    fn miner_active(&mut self) -> Result<bool, ConfigError>;
    fn stop_miner(&mut self) -> Result<(), ConfigError>;
    fn start_miner(&mut self) -> Result<(), ConfigError>;
    fn restart_hugepages(&mut self) -> Result<(), ConfigError>;
}

struct SystemRuntime;

impl RuntimeOps for SystemRuntime {
    fn set_timezone(&mut self, timezone: &str) -> Result<(), ConfigError> {
        run("timedatectl", &["set-timezone", timezone])
    }
    fn set_miner_enabled(&mut self, enabled: bool) -> Result<(), ConfigError> {
        run(
            "systemctl",
            &[
                if enabled { "enable" } else { "disable" },
                "rigos-miner.service",
            ],
        )
    }
    fn miner_active(&mut self) -> Result<bool, ConfigError> {
        let status = Command::new("systemctl")
            .args(["is-active", "--quiet", "rigos-miner.service"])
            .status()
            .map_err(io_failure)?;
        match status.code() {
            Some(0) => Ok(true),
            Some(3) => Ok(false),
            _ => Err(transaction_error(
                "snapshot_active",
                "could not read miner running state",
            )),
        }
    }
    fn stop_miner(&mut self) -> Result<(), ConfigError> {
        run("systemctl", &["stop", "rigos-miner.service"])
    }
    fn start_miner(&mut self) -> Result<(), ConfigError> {
        run("systemctl", &["start", "rigos-miner.service"])
    }
    fn restart_hugepages(&mut self) -> Result<(), ConfigError> {
        run("systemctl", &["restart", "rigos-hugepages.service"])
    }
}

fn current_revision(state: &Path) -> Result<Option<String>, ConfigError> {
    let current = state.join("current");
    let target = match fs::read_link(&current) {
        Ok(target) => target,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_failure(error)),
    };
    let revision = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| transaction_error("current", "current revision pointer is invalid"))?
        .to_owned();
    Uuid::parse_str(&revision)
        .map_err(|_| transaction_error("current", "current revision identifier is invalid"))?;
    if !state.join("current/policy.json").is_file() || !state.join("current/xmrig.json").is_file() {
        return Err(transaction_error(
            "current",
            "current revision target is incomplete",
        ));
    }
    Ok(Some(revision))
}

fn commit_once(state: &Path, request: &CommitRequest) -> Result<(String, bool), ConfigError> {
    if let Some(revision) = current_revision(state)? {
        return Ok((revision, false));
    }
    let revision = commit_revision(state, &request.proposal, &request.identity)
        .map_err(|_| transaction_error("commit", "configuration revision commit failed"))?;
    write_firstboot_result(state, "configuration_committed", &revision, None)?;
    Ok((revision, true))
}

fn activate(
    state: &Path,
    cmdline: &Path,
    runtime: &mut impl RuntimeOps,
) -> Result<(String, bool), ConfigError> {
    let revision = current_revision(state)?
        .ok_or_else(|| transaction_error("preflight", "configuration has not been committed"))?;
    let policy: Policy = read_json(&state.join("current/policy.json"))?;
    let operation = apply_activation_runtime(&policy, cmdline, runtime);
    match operation {
        Ok(started) => {
            write_activation_status(state, "ready", &revision, None)?;
            write_firstboot_result(state, "ready", &revision, None)?;
            Ok((revision, started))
        }
        Err(error) => {
            let _ = runtime.stop_miner();
            let stage = error.diagnostic.key.as_deref().unwrap_or("unknown");
            write_activation_status(state, "activation_failed", &revision, Some(stage))?;
            write_firstboot_result(state, "activation_failed", &revision, Some(stage))?;
            Err(error)
        }
    }
}

fn apply_activation_runtime(
    policy: &Policy,
    cmdline: &Path,
    runtime: &mut impl RuntimeOps,
) -> Result<bool, ConfigError> {
    (|| {
        runtime.stop_miner().map_err(|_| {
            transaction_error("stop_before_activation", "miner could not be stopped")
        })?;
        if runtime.miner_active()? {
            return Err(transaction_error(
                "stop_before_activation",
                "miner remained active before activation",
            ));
        }
        runtime
            .set_timezone(&policy.timezone)
            .map_err(|_| transaction_error("timezone", "timezone apply failed"))?;
        let enabled = policy.miner_start_mode == MinerStartMode::OnBoot;
        runtime
            .set_miner_enabled(enabled)
            .map_err(|_| transaction_error("enabled_state", "miner policy apply failed"))?;
        runtime
            .restart_hugepages()
            .map_err(|_| transaction_error("hugepages", "huge page authority restart failed"))?;
        let started = enabled && !cmdline_blocks_mining(cmdline);
        if started {
            runtime
                .start_miner()
                .map_err(|_| transaction_error("miner_start", "miner start failed"))?;
            if !runtime.miner_active()? {
                return Err(transaction_error(
                    "miner_start",
                    "miner did not become active",
                ));
            }
        }
        Ok(started)
    })()
}

fn write_activation_status(
    state: &Path,
    outcome: &str,
    revision: &str,
    stage: Option<&str>,
) -> Result<(), ConfigError> {
    write_atomic_json(
        &state.join("activation-status.json"),
        &json!({"schema":"rigos.activation-status/v1","outcome":outcome,"revision":revision,"failure_stage":stage}),
    )
}

fn write_firstboot_result(
    state: &Path,
    outcome: &str,
    revision: &str,
    stage: Option<&str>,
) -> Result<(), ConfigError> {
    write_atomic_json(
        &state.join("firstboot-status.json"),
        &json!({"schema":"rigos.firstboot-status/v1","outcome":outcome,"revision":revision,"failure_stage":stage}),
    )
}

fn activation_ready(state: &Path) -> Result<bool, ConfigError> {
    let Some(revision) = current_revision(state)? else {
        return Ok(false);
    };
    let status: Value = match read_json(&state.join("activation-status.json")) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    Ok(
        status.get("schema").and_then(Value::as_str) == Some("rigos.activation-status/v1")
            && status.get("outcome").and_then(Value::as_str) == Some("ready")
            && status.get("revision").and_then(Value::as_str) == Some(revision.as_str()),
    )
}

fn state_allows_config(outcome: &str) -> bool {
    matches!(outcome, "ready" | "grown")
}

fn gate(state: &Path, cmdline: &Path) -> Result<bool, ConfigError> {
    let policy: Policy = read_json(&state.join("current/policy.json"))?;
    let blocked = cmdline_blocks_mining(cmdline);
    Ok(activation_ready(state)?
        && policy.miner_start_mode == MinerStartMode::OnBoot
        && !blocked
        && state.join("current/xmrig.json").is_file())
}

fn cmdline_blocks_mining(cmdline: &Path) -> bool {
    fs::read_to_string(cmdline)
        .unwrap_or_default()
        .split_whitespace()
        .any(|item| item == "rigos.nomine=1" || item == "rigos.stateless=1")
}

fn apply_timezone(timezone: &str) -> Result<(), ConfigError> {
    let zone = Path::new("/usr/share/zoneinfo").join(timezone);
    if !zone.is_file() {
        return Err(diagnostic(
            "RIGOS_CONFIG_INVALID_VALUE",
            "configured timezone is not installed",
        ));
    }
    run("timedatectl", &["set-timezone", timezone])
}

fn transaction_error(stage: &str, message: impl Into<String>) -> ConfigError {
    ConfigError {
        diagnostic: ConfigDiagnostic {
            code: "RIGOS_CONFIG_APPLY_FAILED".into(),
            file: None,
            line: None,
            key: Some(stage.into()),
            message: message.into(),
        },
    }
}

fn read_regular_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, ConfigError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        diagnostic(
            "RIGOS_FLIGHT_SHEET_MISSING",
            "selected flight sheet is missing",
        )
    })?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > maximum as u64
    {
        return Err(diagnostic(
            "RIGOS_FLIGHT_SHEET_INVALID",
            "selected flight sheet is not a bounded regular file",
        ));
    }
    read_bounded(path, maximum)
}
fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, ConfigError> {
    let bytes = fs::read(path).map_err(io_failure)?;
    if bytes.len() > maximum {
        return Err(diagnostic(
            "RIGOS_CONFIG_FILE_TOO_LARGE",
            "input exceeds its size limit",
        ));
    }
    Ok(bytes)
}
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ConfigError> {
    let bytes = read_bounded(path, 2 * 1024 * 1024)?;
    serde_json::from_slice(&bytes).map_err(|_| {
        diagnostic(
            "RIGOS_CONFIG_INVALID_VALUE",
            "required JSON state is invalid",
        )
    })
}
fn write_private_json<T: Serialize>(path: &Path, value: &T) -> Result<(), ConfigError> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(io_failure)?;
    serde_json::to_writer_pretty(file, value)
        .map_err(|_| diagnostic("RIGOS_CONFIG_INVALID_VALUE", "proposal write failed"))
}
fn write_atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<(), ConfigError> {
    let parent = path
        .parent()
        .ok_or_else(|| diagnostic("RIGOS_CONFIG_INVALID_VALUE", "state path has no parent"))?;
    let temporary = parent.join(format!(".status-{}", Uuid::new_v4()));
    write_private_json(&temporary, value)?;
    fs::rename(&temporary, path).map_err(io_failure)?;
    let directory = fs::File::open(parent).map_err(io_failure)?;
    directory.sync_all().map_err(io_failure)
}
fn run(program: &str, arguments: &[&str]) -> Result<(), ConfigError> {
    if Command::new(program)
        .args(arguments)
        .status()
        .map_err(io_failure)?
        .success()
    {
        Ok(())
    } else {
        Err(diagnostic(
            "RIGOS_CONFIG_INVALID_VALUE",
            format!("runtime apply stage {program} failed"),
        ))
    }
}
fn diagnostic(code: &str, message: impl Into<String>) -> ConfigError {
    ConfigError {
        diagnostic: ConfigDiagnostic {
            code: code.into(),
            file: None,
            line: None,
            key: None,
            message: message.into(),
        },
    }
}
fn io_failure(value: io::Error) -> ConfigError {
    diagnostic(
        "RIGOS_CONFIG_INVALID_VALUE",
        format!("I/O operation failed: {value}"),
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    fn attestation() -> Attestation {
        Attestation {
            schema: "rigos.boot-device/v1".into(),
            boot_id: "boot-a".into(),
            verification_outcome: "verified".into(),
            disk: AttestedDisk {
                _path: "/dev/sda".into(),
                major_minor: "8:0".into(),
                ptuuid: "5249474f".into(),
            },
            partition1: AttestedPartition {
                path: "/dev/sda1".into(),
                major_minor: "8:1".into(),
                partuuid: "5249474f-01".into(),
            },
            root: AttestedRoot {
                major_minor: "8:2".into(),
            },
        }
    }

    #[test]
    fn attestation_uses_stable_identity_not_device_path() {
        let expected = attestation();
        let mut renamed = expected.clone();
        renamed.disk._path = "/dev/sdz".into();
        renamed.partition1.path = "/dev/sdz1".into();
        assert!(attestation_identity_matches(&expected, &renamed));
        for change in ["boot", "disk", "ptuuid", "partition", "partuuid", "root"] {
            let mut swapped = expected.clone();
            match change {
                "boot" => swapped.boot_id = "boot-b".into(),
                "disk" => swapped.disk.major_minor = "9:0".into(),
                "ptuuid" => swapped.disk.ptuuid = "different".into(),
                "partition" => swapped.partition1.major_minor = "9:1".into(),
                "partuuid" => swapped.partition1.partuuid = "different-01".into(),
                "root" => swapped.root.major_minor = "9:2".into(),
                _ => unreachable!(),
            }
            assert!(!attestation_identity_matches(&expected, &swapped));
        }
    }

    #[test]
    fn limited_capacity_is_a_negative_gate() {
        assert!(state_allows_config("ready"));
        assert!(state_allows_config("grown"));
        assert!(!state_allows_config("limited_capacity"));
        assert!(!state_allows_config("blocked_layout_mismatch"));
    }

    #[derive(Default)]
    struct FakeRuntime {
        timezone: String,
        enabled: bool,
        active: bool,
        fail_once: Option<&'static str>,
        events: Vec<String>,
    }

    impl FakeRuntime {
        fn fail(&mut self, stage: &'static str) -> Result<(), ConfigError> {
            self.events.push(stage.into());
            if self.fail_once == Some(stage) {
                self.fail_once = None;
                Err(transaction_error(stage, "injected failure"))
            } else {
                Ok(())
            }
        }
    }

    impl RuntimeOps for FakeRuntime {
        fn set_timezone(&mut self, timezone: &str) -> Result<(), ConfigError> {
            self.fail("timezone")?;
            self.timezone = timezone.into();
            Ok(())
        }
        fn set_miner_enabled(&mut self, enabled: bool) -> Result<(), ConfigError> {
            self.fail("enabled_state")?;
            self.enabled = enabled;
            Ok(())
        }
        fn miner_active(&mut self) -> Result<bool, ConfigError> {
            Ok(self.active)
        }
        fn stop_miner(&mut self) -> Result<(), ConfigError> {
            self.fail("stop")?;
            self.active = false;
            Ok(())
        }
        fn start_miner(&mut self) -> Result<(), ConfigError> {
            self.fail("miner_start")?;
            self.active = true;
            Ok(())
        }
        fn restart_hugepages(&mut self) -> Result<(), ConfigError> {
            self.fail("hugepages")
        }
    }

    #[test]
    fn activation_orders_hugepages_before_miner() {
        let policy = Policy {
            timezone: "Asia/Bangkok".into(),
            miner_start_mode: MinerStartMode::OnBoot,
        };
        let mut runtime = FakeRuntime {
            timezone: "UTC".into(),
            enabled: false,
            active: false,
            fail_once: None,
            events: vec![],
        };
        assert!(
            apply_activation_runtime(&policy, Path::new("missing-cmdline"), &mut runtime).unwrap()
        );
        assert_eq!(
            runtime.events,
            [
                "stop",
                "timezone",
                "enabled_state",
                "hugepages",
                "miner_start"
            ]
        );
    }

    #[test]
    fn activation_failure_leaves_miner_stopped() {
        let policy = Policy {
            timezone: "Asia/Bangkok".into(),
            miner_start_mode: MinerStartMode::OnBoot,
        };
        for stage in ["timezone", "enabled_state", "hugepages", "miner_start"] {
            let mut runtime = FakeRuntime {
                timezone: "UTC".into(),
                enabled: false,
                active: false,
                fail_once: Some(stage),
                events: vec![],
            };
            let error =
                apply_activation_runtime(&policy, Path::new("missing-cmdline"), &mut runtime)
                    .unwrap_err();
            assert_eq!(error.diagnostic.key.as_deref(), Some(stage));
            runtime.stop_miner().unwrap();
            assert!(!runtime.active);
        }
    }

    #[cfg(unix)]
    #[test]
    fn activation_retry_preserves_one_current_revision() {
        use std::os::unix::fs::symlink;

        let state = std::env::temp_dir().join(format!("rigos-activation-{}", Uuid::new_v4()));
        let revision = Uuid::new_v4().to_string();
        let revision_path = state.join("revisions").join(&revision);
        fs::create_dir_all(&revision_path).unwrap();
        fs::write(
            revision_path.join("policy.json"),
            br#"{"timezone":"Asia/Bangkok","miner_start_mode":"on_boot"}"#,
        )
        .unwrap();
        fs::write(revision_path.join("xmrig.json"), b"{}\n").unwrap();
        symlink(
            Path::new("revisions").join(&revision),
            state.join("current"),
        )
        .unwrap();

        let mut failed = FakeRuntime {
            timezone: "UTC".into(),
            enabled: false,
            active: false,
            fail_once: Some("hugepages"),
            events: vec![],
        };
        assert!(activate(&state, Path::new("missing-cmdline"), &mut failed).is_err());
        assert!(!failed.active);
        assert_eq!(current_revision(&state).unwrap(), Some(revision.clone()));
        let failed_status: Value = read_json(&state.join("activation-status.json")).unwrap();
        assert_eq!(failed_status["outcome"], "activation_failed");

        let mut retry = FakeRuntime {
            timezone: "UTC".into(),
            enabled: false,
            active: false,
            fail_once: None,
            events: vec![],
        };
        let (activated_revision, started) =
            activate(&state, Path::new("missing-cmdline"), &mut retry).unwrap();
        assert_eq!(activated_revision, revision);
        assert!(started);
        assert_eq!(fs::read_dir(state.join("revisions")).unwrap().count(), 1);
        assert!(activation_ready(&state).unwrap());
        let _ = fs::remove_dir_all(state);
    }
}
