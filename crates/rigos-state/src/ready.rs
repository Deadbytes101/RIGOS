#![forbid(unsafe_code)]

use clap::Parser;
use rigos_state::{
    BootDeviceAttestationV1, StateReadyObservation, StateStatusV1, validate_state_ready,
};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "/run/rigos/state-status.json")]
    status: PathBuf,
    #[arg(long, default_value = "/run/rigos/boot-device.json")]
    attestation: PathBuf,
    #[arg(long, default_value = "/proc/sys/kernel/random/boot_id")]
    boot_id: PathBuf,
    #[arg(long, default_value = "/dev/disk/by-partuuid")]
    partuuid_root: PathBuf,
    #[arg(long, default_value = "/var/lib/rigos")]
    mountpoint: PathBuf,
}

#[derive(Deserialize)]
struct FindmntDocument {
    filesystems: Vec<FindmntEntry>,
}

#[derive(Deserialize)]
struct FindmntEntry {
    source: String,
    #[serde(rename = "maj:min")]
    major_minor: String,
    fstype: String,
    options: String,
    target: String,
}

fn main() -> ExitCode {
    match verify(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rigos-state-ready: {error}");
            ExitCode::FAILURE
        }
    }
}

fn verify(args: &Args) -> Result<(), String> {
    let status: StateStatusV1 = read_json(&args.status)?;
    let attestation: BootDeviceAttestationV1 = read_json(&args.attestation)?;
    let boot_id = fs::read_to_string(&args.boot_id)
        .map_err(|error| error.to_string())?
        .trim()
        .to_owned();
    let partuuid_path = safe_partuuid_path(&args.partuuid_root, &attestation.state.partuuid)?;
    let resolved = fs::canonicalize(&partuuid_path).map_err(|error| error.to_string())?;
    let blkid = output("/usr/sbin/blkid", &["-o", "export", path_text(&resolved)?])?;
    let properties = parse_properties(&blkid);
    let observed_partuuid = properties
        .get("PARTUUID")
        .ok_or_else(|| "mounted state PARTUUID is unavailable".to_owned())?
        .to_owned();
    let label = properties
        .get("LABEL")
        .ok_or_else(|| "mounted state label is unavailable".to_owned())?
        .to_owned();
    let filesystem = properties
        .get("TYPE")
        .ok_or_else(|| "mounted state filesystem is unavailable".to_owned())?
        .to_owned();
    let findmnt: FindmntDocument = serde_json::from_slice(&output_bytes(
        "/usr/bin/findmnt",
        &[
            "--json",
            "--target",
            path_text(&args.mountpoint)?,
            "--output",
            "SOURCE,MAJ:MIN,FSTYPE,OPTIONS,TARGET",
        ],
    )?)
    .map_err(|error| error.to_string())?;
    let mount = findmnt
        .filesystems
        .first()
        .ok_or_else(|| "persistent state is not mounted".to_owned())?;
    if fs::canonicalize(&mount.source).map_err(|error| error.to_string())? != resolved {
        return Err("mount source differs from verified PARTUUID".into());
    }
    let observed = StateReadyObservation {
        boot_id,
        source_major_minor: mount.major_minor.clone(),
        source_partuuid: observed_partuuid,
        filesystem: if mount.fstype == filesystem {
            filesystem
        } else {
            return Err("findmnt and blkid filesystem disagree".into());
        },
        label,
        mountpoint: mount.target.clone(),
        mount_options: mount.options.split(',').map(str::to_owned).collect(),
    };
    validate_state_ready(&status, &attestation, &observed).map_err(str::to_owned)
}

fn safe_partuuid_path(root: &Path, partuuid: &str) -> Result<PathBuf, String> {
    if partuuid.is_empty()
        || partuuid.len() > 128
        || !partuuid
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err("attested state PARTUUID is invalid".into());
    }
    Ok(root.join(partuuid))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    if bytes.len() > 64 * 1024 {
        return Err("runtime state exceeds its size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| "path is not valid UTF-8".to_owned())
}

fn parse_properties(value: &str) -> std::collections::BTreeMap<String, String> {
    value
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn output(program: &str, arguments: &[&str]) -> Result<String, String> {
    String::from_utf8(output_bytes(program, arguments)?).map_err(|error| error.to_string())
}

fn output_bytes(program: &str, arguments: &[&str]) -> Result<Vec<u8>, String> {
    let result = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| error.to_string())?;
    if result.status.success() {
        Ok(result.stdout)
    } else {
        Err(format!("{program} failed"))
    }
}
