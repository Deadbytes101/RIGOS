use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::{FileExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const REGISTER: u64 = 0x1a4;
const TARGET: u64 = 0x0f;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn unique_root(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rigos-{name}-{unique}"))
}

fn write_cpuinfo(path: &Path, model: u32) {
    fs::write(
        path,
        format!(
            "processor : 0\nvendor_id : GenuineIntel\ncpu family : 6\nmodel : {model}\nmodel name : Synthetic CPU\n\n"
        ),
    )
    .unwrap();
}

fn create_msr(path: &Path, value: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    file.seek(SeekFrom::Start(REGISTER)).unwrap();
    file.write_all(&value.to_le_bytes()).unwrap();
    file.sync_all().unwrap();
}

fn read_msr(path: &Path) -> u64 {
    let file = File::open(path).unwrap();
    let mut bytes = [0_u8; 8];
    file.read_exact_at(&mut bytes, REGISTER).unwrap();
    u64::from_le_bytes(bytes)
}

fn run_authority(root: &Path, command: &str) -> std::process::Output {
    let script = repo_path("build/usb/includes.chroot/usr/lib/rigos/rigos-randomx-msr");
    Command::new("/usr/bin/python3")
        .arg(script)
        .arg(command)
        .arg("--cpuinfo")
        .arg(root.join("cpuinfo"))
        .arg("--online")
        .arg(root.join("online"))
        .arg("--device-root")
        .arg(root.join("dev/cpu"))
        .arg("--boot-id")
        .arg(root.join("boot_id"))
        .arg("--status")
        .arg(root.join("run/status.json"))
        .arg("--state")
        .arg(root.join("run/state.json"))
        .arg("--lock")
        .arg(root.join("run/authority.lock"))
        .output()
        .unwrap()
}

fn status(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("run/status.json")).unwrap()).unwrap()
}

#[test]
fn source_wiring_is_optional_reversible_and_narrow() {
    let service = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-randomx-msr.service",
    ))
    .unwrap();
    let dropin = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-miner.service.d/randomx-msr.conf",
    ))
    .unwrap();
    let packages =
        fs::read_to_string(repo_path("build/usb/package-lists/rigos.list.chroot")).unwrap();
    let authority = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/usr/lib/rigos/rigos-randomx-msr",
    ))
    .unwrap();

    for required in [
        "ExecStartPre=-/usr/sbin/modprobe msr",
        "ExecStart=/usr/bin/python3 /usr/lib/rigos/rigos-randomx-msr apply",
        "ExecStop=/usr/bin/python3 /usr/lib/rigos/rigos-randomx-msr restore",
        "CapabilityBoundingSet=CAP_SYS_MODULE CAP_SYS_RAWIO",
        "ReadWritePaths=/run/rigos -/dev/cpu",
    ] {
        assert!(service.contains(required), "service is missing {required}");
    }
    assert!(dropin.contains("Wants=rigos-randomx-msr.service"));
    assert!(dropin.contains("After=rigos-randomx-msr.service"));
    assert!(!dropin.contains("Requires=rigos-randomx-msr.service"));
    assert!(packages.lines().any(|line| line == "kmod"));
    assert!(authority.contains("SUPPORTED_CPUS = {(\"GenuineIntel\", 6, 42)}"));
    assert!(authority.contains("REGISTER = 0x1A4"));
    assert!(authority.contains("TARGET_VALUE = 0xF"));
    assert!(authority.contains("apply_failed_rolled_back"));
    assert!(authority.contains("apply_failed_rollback_incomplete"));
    assert!(authority.contains("stale_state_discarded"));
}

#[test]
fn supported_cpu_apply_is_idempotent_and_restore_recovers_original_values() {
    let root = unique_root("randomx-msr-roundtrip");
    fs::create_dir_all(root.join("run")).unwrap();
    write_cpuinfo(&root.join("cpuinfo"), 42);
    fs::write(root.join("online"), "0-1\n").unwrap();
    fs::write(
        root.join("boot_id"),
        "00000000-0000-0000-0000-000000000001\n",
    )
    .unwrap();
    let cpu0 = root.join("dev/cpu/0/msr");
    let cpu1 = root.join("dev/cpu/1/msr");
    create_msr(&cpu0, 0x10);
    create_msr(&cpu1, 0x20);

    let applied = run_authority(&root, "apply");
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(read_msr(&cpu0), TARGET);
    assert_eq!(read_msr(&cpu1), TARGET);
    let first = status(&root);
    assert_eq!(first["outcome"], "ready");
    assert!(first["reason"].is_null());
    assert_eq!(
        fs::metadata(root.join("run/state.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    let repeated = run_authority(&root, "apply");
    assert!(repeated.status.success());
    let second = status(&root);
    assert_eq!(second["outcome"], "ready");
    assert_eq!(second["reason"], "already_applied");

    let restored = run_authority(&root, "restore");
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    assert_eq!(read_msr(&cpu0), 0x10);
    assert_eq!(read_msr(&cpu1), 0x20);
    assert!(!root.join("run/state.json").exists());
    let final_status = status(&root);
    assert_eq!(final_status["outcome"], "restored");
    assert!(final_status["reason"].is_null());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn unsupported_cpu_is_truthful_and_never_requires_msr_devices() {
    let root = unique_root("randomx-msr-unsupported");
    fs::create_dir_all(root.join("run")).unwrap();
    write_cpuinfo(&root.join("cpuinfo"), 58);
    fs::write(root.join("online"), "0-3\n").unwrap();
    fs::write(
        root.join("boot_id"),
        "00000000-0000-0000-0000-000000000002\n",
    )
    .unwrap();

    let output = run_authority(&root, "apply");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = status(&root);
    assert_eq!(value["outcome"], "unsupported");
    assert_eq!(value["reason"], "cpu_not_allowlisted");
    assert!(!root.join("run/state.json").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn partial_write_failure_rolls_back_every_recoverable_cpu_and_keeps_state() {
    let root = unique_root("randomx-msr-rollback");
    fs::create_dir_all(root.join("run")).unwrap();
    write_cpuinfo(&root.join("cpuinfo"), 42);
    fs::write(root.join("online"), "0-1\n").unwrap();
    fs::write(
        root.join("boot_id"),
        "00000000-0000-0000-0000-000000000003\n",
    )
    .unwrap();
    let cpu0 = root.join("dev/cpu/0/msr");
    create_msr(&cpu0, 0x55);
    fs::create_dir_all(root.join("dev/cpu/1")).unwrap();
    symlink("/dev/full", root.join("dev/cpu/1/msr")).unwrap();

    let output = run_authority(&root, "apply");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(read_msr(&cpu0), 0x55);
    let value = status(&root);
    assert_eq!(value["outcome"], "degraded");
    assert_eq!(value["reason"], "apply_failed_rollback_incomplete");
    assert_eq!(value["rollback"]["attempted"], true);
    assert_eq!(value["rollback"]["complete"], false);
    assert!(root.join("run/state.json").exists());

    let _ = fs::remove_dir_all(root);
}
