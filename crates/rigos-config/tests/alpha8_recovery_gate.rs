use std::fs;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn recovery_gate_accepts_only_current_persisted_credential_truth() {
    let root = std::env::temp_dir().join(format!("rigos-recovery-gate-{}", Uuid::new_v4()));
    let runtime = root.join("run");
    let boot_id = root.join("boot-id");
    fs::create_dir_all(&runtime).unwrap();
    fs::write(&boot_id, "boot-test\n").unwrap();

    let status = runtime.join("recovery-access-status.json");
    let valid = serde_json::json!({
        "schema": "rigos.recovery-access-status/v1",
        "boot_id": "boot-test",
        "local_console_access": true,
        "credential_action": "created",
        "credential_persisted": true
    });
    fs::write(&status, serde_json::to_vec(&valid).unwrap()).unwrap();

    let gate = repo_path("build/usb/includes.chroot/usr/lib/rigos/rigos-recovery-access-verify");
    let run = |runtime_path: &PathBuf, boot_path: &PathBuf| {
        Command::new("python3")
            .arg(&gate)
            .env("RIGOS_RUNTIME_PATH", runtime_path)
            .env("RIGOS_BOOT_ID_PATH", boot_path)
            .status()
            .unwrap()
    };

    assert!(run(&runtime, &boot_id).success());

    let stale = serde_json::json!({
        "schema": "rigos.recovery-access-status/v1",
        "boot_id": "old-boot",
        "local_console_access": true,
        "credential_action": "created",
        "credential_persisted": true
    });
    fs::write(&status, serde_json::to_vec(&stale).unwrap()).unwrap();
    assert_eq!(run(&runtime, &boot_id).code(), Some(2));

    let not_persisted = serde_json::json!({
        "schema": "rigos.recovery-access-status/v1",
        "boot_id": "boot-test",
        "local_console_access": true,
        "credential_action": "created",
        "credential_persisted": false
    });
    fs::write(&status, serde_json::to_vec(&not_persisted).unwrap()).unwrap();
    assert_eq!(run(&runtime, &boot_id).code(), Some(2));

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
