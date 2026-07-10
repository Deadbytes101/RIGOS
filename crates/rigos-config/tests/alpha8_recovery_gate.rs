use std::fs;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn run_gate(runtime: &PathBuf, boot: &PathBuf) -> std::process::ExitStatus {
    let gate = repo_path(
        "build/usb/includes.chroot/usr/lib/rigos/rigos-recovery-access-verify",
    );
    Command::new("python3")
        .arg(gate)
        .env("RIGOS_RUNTIME_PATH", runtime)
        .env("RIGOS_BOOT_ID_PATH", boot)
        .status()
        .unwrap()
}

#[test]
fn recovery_gate_accepts_persistent_or_explicit_boot_scoped_credential_truth() {
    let root = std::env::temp_dir().join(format!("rigos-recovery-gate-{}", Uuid::new_v4()));
    let runtime = root.join("run");
    let boot_id = root.join("boot-id");
    fs::create_dir_all(&runtime).unwrap();
    fs::write(&boot_id, "boot-test\n").unwrap();

    let status = runtime.join("recovery-access-status.json");
    let persistent = serde_json::json!({
        "schema": "rigos.recovery-access-status/v1",
        "boot_id": "boot-test",
        "local_console_access": true,
        "credential_action": "created",
        "credential_scope": "persistent",
        "credential_persisted": true,
        "state_outcome": "ready"
    });
    fs::write(&status, serde_json::to_vec(&persistent).unwrap()).unwrap();
    assert!(run_gate(&runtime, &boot_id).success());

    let boot_scoped = serde_json::json!({
        "schema": "rigos.recovery-access-status/v1",
        "boot_id": "boot-test",
        "local_console_access": true,
        "credential_action": "created",
        "credential_scope": "boot",
        "credential_persisted": false,
        "state_outcome": "repair_required"
    });
    fs::write(&status, serde_json::to_vec(&boot_scoped).unwrap()).unwrap();
    assert!(run_gate(&runtime, &boot_id).success());

    let stale = serde_json::json!({
        "schema": "rigos.recovery-access-status/v1",
        "boot_id": "old-boot",
        "local_console_access": true,
        "credential_action": "created",
        "credential_scope": "persistent",
        "credential_persisted": true,
        "state_outcome": "ready"
    });
    fs::write(&status, serde_json::to_vec(&stale).unwrap()).unwrap();
    assert_eq!(run_gate(&runtime, &boot_id).code(), Some(2));

    let missing_persistent_store = serde_json::json!({
        "schema": "rigos.recovery-access-status/v1",
        "boot_id": "boot-test",
        "local_console_access": true,
        "credential_action": "created",
        "credential_scope": "persistent",
        "credential_persisted": false,
        "state_outcome": "ready"
    });
    fs::write(
        &status,
        serde_json::to_vec(&missing_persistent_store).unwrap(),
    )
    .unwrap();
    assert_eq!(run_gate(&runtime, &boot_id).code(), Some(2));

    let false_boot_claim = serde_json::json!({
        "schema": "rigos.recovery-access-status/v1",
        "boot_id": "boot-test",
        "local_console_access": true,
        "credential_action": "created",
        "credential_scope": "boot",
        "credential_persisted": true,
        "state_outcome": "limited_capacity"
    });
    fs::write(&status, serde_json::to_vec(&false_boot_claim).unwrap()).unwrap();
    assert_eq!(run_gate(&runtime, &boot_id).code(), Some(2));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn recovery_service_accepts_legacy_exit_one_only_with_post_validation() {
    let unit = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-recovery-access.service",
    ))
    .unwrap();

    assert!(unit.contains(
        "Before=rigos-state-ready.service rigos-firstboot.service getty@tty1.service ssh.service"
    ));
    assert!(unit.contains("SuccessExitStatus=1"));
    assert!(
        unit.contains("ExecStartPost=/usr/bin/python3 /usr/lib/rigos/rigos-recovery-access-verify")
    );
}
