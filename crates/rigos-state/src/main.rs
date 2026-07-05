#![forbid(unsafe_code)]

use chrono::{SecondsFormat, Utc};
use clap::Parser;
use fs2::FileExt;
use rigos_schema::{ImageLayoutV1, STATE_LAYOUT_SCHEMA, StateLayoutV1};
use rigos_state::{LayoutError, LsblkDocument, StateOutcome, VerifiedLayout, validate_layout};
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
    #[arg(long)]
    dry_run: bool,
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

fn main() -> ExitCode {
    let args = Args::parse();
    let (outcome, message) = match execute(&args) {
        Ok(outcome) => (outcome, None),
        Err(
            error @ (InitError::Layout(LayoutError::AmbiguousBootDevice) | InitError::Discovery(_)),
        ) => (
            StateOutcome::BlockedAmbiguousBootDevice,
            Some(error.to_string()),
        ),
        Err(error @ InitError::Layout(_)) => {
            (StateOutcome::BlockedLayoutMismatch, Some(error.to_string()))
        }
        Err(error) => (StateOutcome::LimitedCapacity, Some(error.to_string())),
    };
    let _ = write_atomic(
        &args.status,
        &json!({"schema":"rigos.state-status/v1","outcome":outcome,"message":message}),
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
    let lsblk_raw = run(
        "lsblk",
        &[
            "--json",
            "--bytes",
            "--paths",
            "--output",
            "MAJ:MIN,PATH,TYPE,SIZE,RO,TRAN,PARTN,PARTTYPE,PARTUUID,PARTLABEL,START,MOUNTPOINTS,FSTYPE,LABEL",
        ],
        None,
        &[0],
    )?;
    let observed: LsblkDocument = serde_json::from_slice(&lsblk_raw)?;
    let verified = validate_layout(&manifest, &observed, &boot_major_minor)?;
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
    if args.dry_run {
        return Ok(StateOutcome::Ready);
    }

    let disk = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&verified.disk_path)?;
    disk.try_lock_exclusive()
        .map_err(|e| InitError::Discovery(format!("exclusive disk lock failed: {e}")))?;
    let sector_size = u64::from(manifest.logical_sector_size);
    let disk_sectors = verified.disk_size_bytes / sector_size;
    let last_usable = disk_sectors
        .checked_sub(34)
        .ok_or_else(|| InitError::Discovery("invalid disk geometry".into()))?;
    let aligned_end = ((last_usable + 1) / manifest.alignment_lba) * manifest.alignment_lba - 1;
    let intended_size = aligned_end
        .checked_sub(verified.state_start_lba)
        .and_then(|v| v.checked_add(1))
        .ok_or_else(|| InitError::Discovery("invalid state geometry".into()))?;
    let mut grown = false;
    if intended_size > verified.state_size_lba {
        run("sgdisk", &["--verify", &verified.disk_path], None, &[0])?;
        run(
            "sgdisk",
            &["--move-second-header", &verified.disk_path],
            None,
            &[0],
        )?;
        let line = format!(
            "start={}, size={}, type={}, uuid={}, name=RIGOS_STATE_SEED\n",
            verified.state_start_lba,
            intended_size,
            verified.state_type_guid,
            verified.state_unique_guid
        );
        run(
            "sfdisk",
            &["--no-reread", "--force", "-N", "5", &verified.disk_path],
            Some(line.as_bytes()),
            &[0],
        )?;
        run(
            "partx",
            &["--update", "--nr", "5", &verified.disk_path],
            None,
            &[0],
        )?;
        run("udevadm", &["settle", "--timeout=10"], None, &[0])?;
        let refreshed: LsblkDocument = serde_json::from_slice(&run(
            "lsblk",
            &[
                "--json",
                "--bytes",
                "--paths",
                "--output",
                "MAJ:MIN,PATH,TYPE,SIZE,RO,TRAN,PARTN,PARTTYPE,PARTUUID,PARTLABEL,START,MOUNTPOINTS,FSTYPE,LABEL",
            ],
            None,
            &[0],
        )?)?;
        let refreshed = validate_layout(&manifest, &refreshed, &boot_major_minor)?;
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
    initialize_state(&manifest, &verified, intended_size, &args.mountpoint)?;
    FileExt::unlock(&disk)?;
    Ok(if grown {
        StateOutcome::Grown
    } else {
        StateOutcome::Ready
    })
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
