#![forbid(unsafe_code)]

use chrono::{SecondsFormat, Utc};
use clap::Parser;
use fs2::FileExt;
use rigos_schema::{ImageLayoutV1, STATE_LAYOUT_SCHEMA, StateLayoutV1};
use rigos_state::{
    LayoutError, LsblkDocument, SfdiskDocument, StateOutcome, VerifiedLayout, boot_parent_disk,
    validate_layout, validate_layout_for_attestation,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "/usr/lib/rigos/image-layout.json")]
    manifest: PathBuf,
    #[arg(long, default_value = "/run/live/medium")]
    live_medium: PathBuf,
    #[arg(long, default_value = "/var/lib/rigos")]
    mountpoint: PathBuf,
    #[arg(long, default_value = "/run/rigos/state-status.json")]
    status: PathBuf,
    #[arg(long, default_value = "/run/rigos/boot-device.json")]
    attestation: PathBuf,
    #[arg(long, default_value = "/dev/disk/by-partuuid")]
    partuuid_root: PathBuf,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    attestation_only: bool,
}

#[derive(Deserialize)]
struct FindmntDocument {
    filesystems: Vec<FindmntEntry>,
}

#[derive(Deserialize)]
struct FindmntEntry {
    #[serde(rename = "maj:min")]
    major_minor: String,
}

#[derive(Deserialize)]
struct StateMountDocument {
    filesystems: Vec<StateMountEntry>,
}

#[derive(Deserialize)]
struct StateMountEntry {
    source: String,
    #[serde(rename = "maj:min")]
    major_minor: String,
    fstype: String,
    options: String,
    target: String,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let _ = fs::remove_file(&args.attestation);
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .unwrap_or_default()
        .trim()
        .to_owned();
    let (outcome, action, message) = match execute(&args) {
        Ok(StateOutcome::Grown) => (StateOutcome::Ready, Some("grown"), None),
        Ok(StateOutcome::Ready) => (StateOutcome::Ready, Some("unchanged"), None),
        Ok(outcome) => (outcome, None, None),
        Err(
            error @ (InitError::Layout(LayoutError::AmbiguousBootDevice) | InitError::Discovery(_)),
        ) => (
            StateOutcome::BlockedAmbiguousBootDevice,
            None,
            Some(error.to_string()),
        ),
        Err(error @ InitError::Layout(_)) => (
            StateOutcome::BlockedLayoutMismatch,
            None,
            Some(error.to_string()),
        ),
        Err(error) => (StateOutcome::LimitedCapacity, None, Some(error.to_string())),
    };
    let attestation: Option<serde_json::Value> = fs::read(&args.attestation)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let device = attestation
        .as_ref()
        .and_then(|value| value.pointer("/state/path"))
        .and_then(serde_json::Value::as_str);
    let partuuid = attestation
        .as_ref()
        .and_then(|value| value.pointer("/state/partuuid"))
        .and_then(serde_json::Value::as_str);
    let _ = write_atomic(
        &args.status,
        &json!({
            "schema":"rigos.state-status/v1",
            "boot_id":boot_id,
            "outcome":outcome,
            "action":action,
            "message":message,
            "device":device,
            "partuuid":partuuid,
            "mountpoint":if outcome == StateOutcome::Ready { Some(path_str(&args.mountpoint).unwrap_or("/var/lib/rigos")) } else { None },
        }),
    );
    println!(
        "{}",
        serde_json::to_string(&json!({"outcome":outcome,"message":message})).unwrap()
    );
    ExitCode::SUCCESS
}

#[derive(Debug, thiserror::Error)]
enum InitError {
    #[error("I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON failure: {0}")]
    Json(#[from] serde_json::Error),
    #[error("layout rejected: {0}")]
    Layout(#[from] LayoutError),
    #[error("boot-device discovery failed: {0}")]
    Discovery(String),
    #[error("bounded command failed: {0}")]
    Command(String),
}

fn execute(args: &Args) -> Result<StateOutcome, InitError> {
    if fs::read_to_string("/proc/cmdline")
        .unwrap_or_default()
        .split_whitespace()
        .any(|value| value == "rigos.stateless=1")
    {
        return Ok(StateOutcome::Stateless);
    }

    let manifest: ImageLayoutV1 = serde_json::from_str(&fs::read_to_string(&args.manifest)?)?;
    let findmnt: FindmntDocument = serde_json::from_slice(&run(
        "findmnt",
        &[
            "--json",
            "--target",
            path_str(&args.live_medium)?,
            "--output",
            "MAJ:MIN",
        ],
        None,
        &[0],
    )?)?;
    let boot_major_minor = findmnt
        .filesystems
        .first()
        .ok_or_else(|| InitError::Discovery("live medium has no block identity".into()))?
        .major_minor
        .clone();

    let observed = read_lsblk()?;
    let boot_disk_path = boot_parent_disk(&observed, &boot_major_minor)
        .map(|device| device.path.clone())
        .ok_or_else(|| InitError::Discovery("boot parent disk was not found".into()))?;
    let table = read_sfdisk(&boot_disk_path)?;
    let mut verified = if args.attestation_only {
        validate_layout_for_attestation(
            &manifest,
            &observed,
            &table,
            &boot_major_minor,
            path_str(&args.mountpoint)?,
        )?
    } else {
        validate_layout(&manifest, &observed, &table, &boot_major_minor)?
    };

    let udev = String::from_utf8_lossy(&run(
        "udevadm",
        &["info", "--query=property", "--name", &verified.disk_path],
        None,
        &[0],
    )?)
    .into_owned();
    if !udev.lines().any(|line| line == "ID_BUS=usb") {
        return Err(LayoutError::NotWritableUsb.into());
    }
    write_atomic(
        &args.attestation,
        &json!({
            "schema":"rigos.boot-device/v1",
            "boot_id":fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim(),
            "verification_outcome":"verified",
            "disk":{
                "path":verified.disk_path,
                "major_minor":verified.disk_major_minor,
                "ptuuid":verified.disk_ptuuid,
            },
            "partition1":{
                "path":verified.efi_path,
                "major_minor":verified.efi_major_minor,
                "partuuid":verified.efi_partuuid,
            },
            "root":{"major_minor":verified.root_major_minor},
            "state":{
                "path":verified.state_path,
                "major_minor":verified.state_major_minor,
                "partuuid":verified.state_unique_guid,
            },
        }),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&args.attestation, fs::Permissions::from_mode(0o600))?;
    }
    if args.dry_run {
        return Ok(StateOutcome::Ready);
    }

    let resolved_state = wait_for_verified_state_device(
        &args.partuuid_root,
        &verified.state_unique_guid,
        Duration::from_secs(10),
    )?;
    validate_exact_state_device(&resolved_state, &verified)?;
    verified.state_path = path_str(&resolved_state)?.to_owned();

    let disk = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&verified.disk_path)?;
    disk.try_lock_exclusive()
        .map_err(|error| InitError::Discovery(format!("exclusive disk lock failed: {error}")))?;

    let sector_size = u64::from(manifest.logical_sector_size);
    let disk_sectors = verified.disk_size_bytes / sector_size;
    let aligned_sector_count = (disk_sectors / manifest.alignment_lba) * manifest.alignment_lba;
    let aligned_end = aligned_sector_count
        .checked_sub(1)
        .ok_or_else(|| InitError::Discovery("invalid disk geometry".into()))?;
    let intended_size = aligned_end
        .checked_sub(verified.state_start_lba)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| InitError::Discovery("invalid state geometry".into()))?;

    let mut grown = false;
    if intended_size > verified.state_size_lba {
        let partition_type = verified.state_type_guid.trim_start_matches("0x");
        let line = format!(
            "start={}, size={}, type={}\n",
            verified.state_start_lba, intended_size, partition_type
        );
        run(
            "sfdisk",
            &["--no-reread", "--force", "-N", "4", &verified.disk_path],
            Some(line.as_bytes()),
            &[0],
        )?;
        run(
            "partx",
            &["--update", "--nr", "4", &verified.disk_path],
            None,
            &[0],
        )?;
        let resolved_state = wait_for_verified_state_device(
            &args.partuuid_root,
            &verified.state_unique_guid,
            Duration::from_secs(10),
        )?;

        let refreshed_devices = read_lsblk()?;
        let refreshed_table = read_sfdisk(&verified.disk_path)?;
        let refreshed = validate_layout(
            &manifest,
            &refreshed_devices,
            &refreshed_table,
            &boot_major_minor,
        )?;
        if refreshed.state_start_lba != verified.state_start_lba
            || refreshed.state_size_lba != intended_size
            || !refreshed
                .state_unique_guid
                .eq_ignore_ascii_case(&verified.state_unique_guid)
        {
            return Err(InitError::Discovery(
                "post-grow geometry did not match the intended monotonic update".into(),
            ));
        }
        validate_exact_state_device(&resolved_state, &refreshed)?;
        verified.state_path = path_str(&resolved_state)?.to_owned();
        grown = true;
    }

    run("e2fsck", &["-p", &verified.state_path], None, &[0, 1])?;
    run("resize2fs", &[&verified.state_path], None, &[0])?;
    let blkid = String::from_utf8_lossy(&run(
        "blkid",
        &["-o", "export", &verified.state_path],
        None,
        &[0],
    )?)
    .into_owned();
    let initialized = blkid.lines().any(|line| line == "LABEL=RIGOS_STATE");
    if !initialized {
        run(
            "tune2fs",
            &["-U", "random", "-L", "RIGOS_STATE", &verified.state_path],
            None,
            &[0],
        )?;
    }

    fs::create_dir_all(&args.mountpoint)?;
    if !mountpoint(&args.mountpoint)? {
        run(
            "mount",
            &[
                "-o",
                "noatime,nodev,nosuid,noexec",
                &verified.state_path,
                path_str(&args.mountpoint)?,
            ],
            None,
            &[0],
        )?;
    }
    verify_state_mount(&verified, &args.mountpoint)?;
    initialize_state(&manifest, &verified, intended_size, &args.mountpoint)?;
    FileExt::unlock(&disk)?;
    Ok(if grown {
        StateOutcome::Grown
    } else {
        StateOutcome::Ready
    })
}

fn verify_state_mount(verified: &VerifiedLayout, mountpoint: &Path) -> Result<(), InitError> {
    let document: StateMountDocument = serde_json::from_slice(&run(
        "findmnt",
        &[
            "--json",
            "--target",
            path_str(mountpoint)?,
            "--output",
            "SOURCE,MAJ:MIN,FSTYPE,OPTIONS,TARGET",
        ],
        None,
        &[0],
    )?)?;
    let mount = document
        .filesystems
        .first()
        .ok_or_else(|| InitError::Discovery("persistent state is not mounted".into()))?;
    let source = fs::canonicalize(&mount.source)?;
    let expected = fs::canonicalize(&verified.state_path)?;
    let required = ["rw", "nosuid", "nodev", "noexec", "noatime"];
    let options: std::collections::BTreeSet<_> = mount.options.split(',').collect();
    if source != expected
        || mount.major_minor != verified.state_major_minor
        || mount.fstype != "ext4"
        || mount.target != path_str(mountpoint)?
        || required.iter().any(|value| !options.contains(value))
    {
        return Err(InitError::Discovery(
            "mounted state does not match the verified identity and options".into(),
        ));
    }
    Ok(())
}

fn wait_for_verified_state_device(
    root: &Path,
    partuuid: &str,
    timeout: Duration,
) -> Result<PathBuf, InitError> {
    if partuuid.is_empty()
        || partuuid.len() > 128
        || !partuuid
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err(InitError::Discovery(
            "verified state PARTUUID is invalid".into(),
        ));
    }
    let candidate = root.join(partuuid);
    let deadline = Instant::now() + timeout;
    loop {
        if candidate.symlink_metadata().is_ok() {
            return fs::canonicalize(&candidate).map_err(InitError::Io);
        }
        if Instant::now() >= deadline {
            return Err(InitError::Discovery(
                "verified state PARTUUID did not appear before timeout".into(),
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn validate_exact_state_device(
    resolved: &Path,
    verified: &VerifiedLayout,
) -> Result<(), InitError> {
    let expected = fs::canonicalize(&verified.state_path)?;
    if expected != resolved {
        return Err(InitError::Discovery(
            "verified PARTUUID resolved to another block device".into(),
        ));
    }
    let observed = read_lsblk()?;
    let child = observed
        .blockdevices
        .iter()
        .find(|disk| disk.major_minor == verified.disk_major_minor)
        .and_then(|disk| {
            disk.children.iter().find(|child| {
                child.major_minor == verified.state_major_minor
                    && child.partuuid.as_deref().is_some_and(|value| {
                        value.eq_ignore_ascii_case(&verified.state_unique_guid)
                    })
            })
        })
        .ok_or_else(|| InitError::Discovery("verified state block identity changed".into()))?;
    if fs::canonicalize(&child.path)? != resolved {
        return Err(InitError::Discovery(
            "lsblk state path differs from verified PARTUUID".into(),
        ));
    }
    let properties = String::from_utf8_lossy(&run(
        "blkid",
        &["-o", "export", path_str(resolved)?],
        None,
        &[0],
    )?)
    .into_owned();
    let property = |name: &str| {
        properties
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}=")))
    };
    if property("TYPE") != Some("ext4")
        || !matches!(property("LABEL"), Some("RIGOS_STATE_SEED" | "RIGOS_STATE"))
        || !property("PARTUUID")
            .is_some_and(|value| value.eq_ignore_ascii_case(&verified.state_unique_guid))
    {
        return Err(InitError::Discovery(
            "verified state filesystem identity changed".into(),
        ));
    }
    Ok(())
}

fn read_lsblk() -> Result<LsblkDocument, InitError> {
    let raw = run(
        "/usr/bin/python3",
        &[
            "/usr/lib/rigos/lsblk-compat",
            "--json",
            "--bytes",
            "--paths",
            "--tree",
            "--output",
            "MAJ:MIN,PATH,TYPE,SIZE,RO,TRAN,PARTN,PARTTYPE,PARTUUID,PARTLABEL,START,PTTYPE,PTUUID,MOUNTPOINTS,FSTYPE,LABEL",
        ],
        None,
        &[0],
    )?;
    Ok(serde_json::from_slice(&raw)?)
}

fn read_sfdisk(disk_path: &str) -> Result<SfdiskDocument, InitError> {
    let raw = run("sfdisk", &["--json", disk_path], None, &[0])?;
    Ok(serde_json::from_slice(&raw)?)
}

fn initialize_state(
    manifest: &ImageLayoutV1,
    verified: &VerifiedLayout,
    size_lba: u64,
    mountpoint: &Path,
) -> Result<(), InitError> {
    fs::create_dir_all(mountpoint)?;
    let node_path = mountpoint.join("node-id");
    if !node_path.exists() {
        write_atomic(
            &node_path,
            &json!({"schema":"rigos.node-identity/v1","id":Uuid::new_v4()}),
        )?;
    }
    let uuid = String::from_utf8_lossy(&run(
        "blkid",
        &["-s", "UUID", "-o", "value", &verified.state_path],
        None,
        &[0],
    )?)
    .trim()
    .to_owned();
    let record = StateLayoutV1 {
        schema: STATE_LAYOUT_SCHEMA.into(),
        image_version: manifest.image_version.clone(),
        initialization_state: "initialized".into(),
        partition_number: manifest.final_state_partition,
        partition_start_lba: verified.state_start_lba,
        partition_end_lba: verified.state_start_lba + size_lba - 1,
        filesystem_type: "ext4".into(),
        filesystem_uuid: uuid,
        state_capacity_bytes: size_lba * u64::from(manifest.logical_sector_size),
        authoritative_image_commit: manifest.build_commit.clone(),
        initialized_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    };
    write_atomic(&mountpoint.join("state-layout.json"), &record)?;
    Ok(())
}

fn mountpoint(path: &Path) -> Result<bool, InitError> {
    Ok(Command::new("mountpoint")
        .arg("-q")
        .arg(path)
        .status()?
        .success())
}

fn path_str(path: &Path) -> Result<&str, InitError> {
    path.to_str()
        .ok_or_else(|| InitError::Discovery("non-UTF-8 path".into()))
}

fn run(
    program: &str,
    args: &[&str],
    input: Option<&[u8]>,
    accepted: &[i32],
) -> Result<Vec<u8>, InitError> {
    let temp = std::env::temp_dir().join(format!("rigos-state-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp)?;
    let stdout_path = temp.join("stdout");
    let stderr_path = temp.join("stderr");
    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::from(File::create(&stdout_path)?))
        .stderr(Stdio::from(File::create(&stderr_path)?));
    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command.spawn()?;
    if let Some(bytes) = input {
        child
            .stdin
            .take()
            .ok_or_else(|| InitError::Command(format!("{program}: stdin unavailable")))?
            .write_all(bytes)?;
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(InitError::Command(format!("{program}: timeout")));
        }
        thread::sleep(Duration::from_millis(50));
    };
    let stdout = read_bounded(&stdout_path)?;
    let stderr = String::from_utf8_lossy(&read_bounded(&stderr_path)?).into_owned();
    let _ = fs::remove_dir_all(temp);
    if !accepted.contains(&status.code().unwrap_or(-1)) {
        return Err(InitError::Command(format!(
            "{program}: exit {:?}: {stderr}",
            status.code()
        )));
    }
    Ok(stdout)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, InitError> {
    let data = fs::read(path)?;
    if data.len() > 1_048_576 {
        return Err(InitError::Command("command output exceeded 1 MiB".into()));
    }
    Ok(data)
}

fn write_atomic<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), InitError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}
