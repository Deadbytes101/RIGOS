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
    Transact {
        #[arg(long, default_value = "/var/lib/rigos")]
        state: PathBuf,
        #[arg(long, default_value = "/proc/cmdline")]
        cmdline: PathBuf,
    },
    Recover {
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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RuntimeSnapshot {
    schema: String,
    boot_id: String,
    previous_revision: Option<PathBuf>,
    timezone: String,
    miner_enabled: bool,
    miner_active: bool,
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
        Commands::Transact { state, cmdline } => {
            let request = read_commit_request()?;
            let (revision, miner_started) =
                transact(&state, &cmdline, &request, &mut SystemRuntime)?;
            Ok(json!({"outcome":"applied","revision":revision,"miner_started":miner_started}))
        }
        Commands::Recover { state } => {
            recover_pending(&state, &mut SystemRuntime)?;
            Ok(json!({"outcome":"recovered"}))
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
    fn timezone(&mut self) -> Result<String, ConfigError>;
    fn set_timezone(&mut self, timezone: &str) -> Result<(), ConfigError>;
    fn miner_enabled(&mut self) -> Result<bool, ConfigError>;
    fn set_miner_enabled(&mut self, enabled: bool) -> Result<(), ConfigError>;
    fn miner_active(&mut self) -> Result<bool, ConfigError>;
    fn stop_miner(&mut self) -> Result<(), ConfigError>;
    fn start_miner(&mut self) -> Result<(), ConfigError>;
}

struct SystemRuntime;

impl RuntimeOps for SystemRuntime {
    fn timezone(&mut self) -> Result<String, ConfigError> {
        output("timedatectl", &["show", "--property=Timezone", "--value"])
    }
    fn set_timezone(&mut self, timezone: &str) -> Result<(), ConfigError> {
        run("timedatectl", &["set-timezone", timezone])
    }
    fn miner_enabled(&mut self) -> Result<bool, ConfigError> {
        let status = Command::new("systemctl")
            .args(["is-enabled", "--quiet", "rigos-miner.service"])
            .status()
            .map_err(io_failure)?;
        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(transaction_error(
                "snapshot_enabled",
                "could not read miner enabled state",
            )),
        }
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
}

fn transact(
    state: &Path,
    cmdline: &Path,
    request: &CommitRequest,
    runtime: &mut impl RuntimeOps,
) -> Result<(String, bool), ConfigError> {
    let pending = state.join(".pending-transaction.json");
    if pending.exists() {
        return Err(transaction_error(
            "snapshot",
            "an incomplete configuration transaction already exists",
        ));
    }
    let snapshot = RuntimeSnapshot {
        schema: "rigos.runtime-snapshot/v1".into(),
        boot_id: fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map_err(io_failure)?
            .trim()
            .to_owned(),
        previous_revision: fs::read_link(state.join("current")).ok(),
        timezone: runtime.timezone()?,
        miner_enabled: runtime.miner_enabled()?,
        miner_active: runtime.miner_active()?,
    };
    write_private_json(&pending, &snapshot)?;
    let operation = (|| {
        runtime.stop_miner().map_err(|_| {
            transaction_error(
                "stop_before_commit",
                "miner could not be stopped before commit",
            )
        })?;
        if runtime.miner_active()? {
            return Err(transaction_error(
                "stop_before_commit",
                "miner remained active before commit",
            ));
        }
        let revision =
            commit_revision(state, &request.proposal, &request.identity).map_err(|_| {
                transaction_error("pointer_swap", "configuration revision commit failed")
            })?;
        let started = apply_new_runtime(&request.proposal.profile, cmdline, runtime)?;
        Ok((revision, started))
    })();
    match operation {
        Ok(result) => {
            fs::remove_file(&pending).map_err(io_failure)?;
            Ok(result)
        }
        Err(failure) => {
            let rollback = rollback_transaction(state, &snapshot, runtime);
            if rollback.is_ok() {
                let _ = fs::remove_file(&pending);
            }
            match rollback {
                Ok(()) => Err(failure),
                Err(rollback_failure) => Err(transaction_error(
                    failure.diagnostic.key.as_deref().unwrap_or("unknown"),
                    format!(
                        "{}; rollback failed at {}",
                        failure.diagnostic.message,
                        rollback_failure
                            .diagnostic
                            .key
                            .as_deref()
                            .unwrap_or("unknown")
                    ),
                )),
            }
        }
    }
}

fn apply_new_runtime(
    profile: &rigos_config::RigProfile,
    cmdline: &Path,
    runtime: &mut impl RuntimeOps,
) -> Result<bool, ConfigError> {
    runtime
        .set_timezone(&profile.timezone)
        .map_err(|_| transaction_error("timezone", "timezone apply failed"))?;
    let enabled = profile.miner_start_mode == MinerStartMode::OnBoot;
    runtime
        .set_miner_enabled(enabled)
        .map_err(|_| transaction_error("enabled_state", "miner enabled state apply failed"))?;
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
}

fn state_allows_config(outcome: &str) -> bool {
    matches!(outcome, "ready" | "grown")
}

fn rollback_transaction(
    state: &Path,
    snapshot: &RuntimeSnapshot,
    runtime: &mut impl RuntimeOps,
) -> Result<(), ConfigError> {
    let mut first_error = None;
    if runtime.stop_miner().is_err() {
        first_error = Some(transaction_error(
            "rollback_stop",
            "miner could not be stopped",
        ));
    }
    if first_error.is_none()
        && restore_pointer(state, snapshot.previous_revision.as_deref()).is_err()
    {
        first_error = Some(transaction_error(
            "rollback_pointer",
            "previous revision could not be restored",
        ));
    }
    let same_boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map(|value| value.trim() == snapshot.boot_id)
        .unwrap_or(false);
    if first_error.is_none() {
        if let Err(error) = restore_runtime(snapshot, same_boot, runtime) {
            first_error = Some(error);
        }
    }
    if let Some(error) = first_error {
        let _ = runtime.stop_miner();
        return Err(error);
    }
    Ok(())
}

fn restore_runtime(
    snapshot: &RuntimeSnapshot,
    restore_running: bool,
    runtime: &mut impl RuntimeOps,
) -> Result<(), ConfigError> {
    runtime.set_timezone(&snapshot.timezone).map_err(|_| {
        transaction_error(
            "rollback_timezone",
            "previous timezone could not be restored",
        )
    })?;
    runtime
        .set_miner_enabled(snapshot.miner_enabled)
        .map_err(|_| {
            transaction_error(
                "rollback_enabled_state",
                "previous enabled state could not be restored",
            )
        })?;
    if restore_running && snapshot.miner_active {
        runtime.start_miner().map_err(|_| {
            transaction_error(
                "rollback_running_state",
                "previous running state could not be restored",
            )
        })?;
    }
    Ok(())
}

fn recover_pending(state: &Path, runtime: &mut impl RuntimeOps) -> Result<(), ConfigError> {
    let pending = state.join(".pending-transaction.json");
    if !pending.is_file() {
        return Ok(());
    }
    let snapshot: RuntimeSnapshot = read_json(&pending)?;
    rollback_transaction(state, &snapshot, runtime)?;
    fs::remove_file(pending).map_err(io_failure)
}

fn restore_pointer(state: &Path, previous: Option<&Path>) -> Result<(), ConfigError> {
    if let Some(previous) = previous {
        let temporary = state.join(format!(".rollback-{}", Uuid::new_v4()));
        create_symlink(previous, &temporary)?;
        fs::rename(temporary, state.join("current")).map_err(io_failure)?;
    } else {
        for name in [
            "current",
            "policy.json",
            "xmrig.json",
            "flight-sheets",
            "identities",
            "external-identity-map.json",
        ] {
            let _ = fs::remove_file(state.join(name));
        }
    }
    Ok(())
}

fn gate(state: &Path, cmdline: &Path) -> Result<bool, ConfigError> {
    let policy: Policy = read_json(&state.join("current/policy.json"))?;
    let blocked = cmdline_blocks_mining(cmdline);
    Ok(policy.miner_start_mode == MinerStartMode::OnBoot
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

fn output(program: &str, arguments: &[&str]) -> Result<String, ConfigError> {
    let result = Command::new(program)
        .args(arguments)
        .output()
        .map_err(io_failure)?;
    if !result.status.success() {
        return Err(transaction_error(
            "runtime_query",
            format!("runtime query {program} failed"),
        ));
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
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
#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> Result<(), ConfigError> {
    std::os::unix::fs::symlink(target, link).map_err(io_failure)
}
#[cfg(not(unix))]
fn create_symlink(_target: &Path, _link: &Path) -> Result<(), ConfigError> {
    Err(diagnostic(
        "RIGOS_CONFIG_INVALID_VALUE",
        "rollback requires Unix symlinks",
    ))
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
        fn timezone(&mut self) -> Result<String, ConfigError> {
            Ok(self.timezone.clone())
        }
        fn set_timezone(&mut self, timezone: &str) -> Result<(), ConfigError> {
            self.fail("timezone")?;
            self.timezone = timezone.into();
            Ok(())
        }
        fn miner_enabled(&mut self) -> Result<bool, ConfigError> {
            Ok(self.enabled)
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
    }

    #[test]
    fn runtime_failure_stages_restore_previous_side_effects() {
        let profile = rigos_config::RigProfile {
            node_name: "rig01".into(),
            timezone: "Asia/Bangkok".into(),
            flight_source: FlightSource::Interactive,
            flight_ref: None,
            miner_start_mode: MinerStartMode::OnBoot,
        };
        let snapshot = RuntimeSnapshot {
            schema: "rigos.runtime-snapshot/v1".into(),
            boot_id: "same".into(),
            previous_revision: None,
            timezone: "UTC".into(),
            miner_enabled: false,
            miner_active: true,
        };
        for stage in ["timezone", "enabled_state", "miner_start"] {
            let mut runtime = FakeRuntime {
                timezone: "UTC".into(),
                enabled: false,
                active: false,
                fail_once: Some(stage),
                events: vec![],
            };
            let error = apply_new_runtime(&profile, Path::new("missing-cmdline"), &mut runtime)
                .unwrap_err();
            assert_eq!(error.diagnostic.key.as_deref(), Some(stage));
            restore_runtime(&snapshot, true, &mut runtime).unwrap();
            assert_eq!(runtime.timezone, "UTC");
            assert!(!runtime.enabled);
            assert!(runtime.active);
        }
    }

    #[test]
    fn rollback_failure_forces_miner_stopped() {
        let snapshot = RuntimeSnapshot {
            schema: "rigos.runtime-snapshot/v1".into(),
            boot_id: "same".into(),
            previous_revision: None,
            timezone: "UTC".into(),
            miner_enabled: true,
            miner_active: true,
        };
        let mut runtime = FakeRuntime {
            timezone: "Asia/Bangkok".into(),
            enabled: true,
            active: true,
            fail_once: Some("timezone"),
            events: vec![],
        };
        assert!(restore_runtime(&snapshot, true, &mut runtime).is_err());
        runtime.stop_miner().unwrap();
        assert!(!runtime.active);
    }
}
