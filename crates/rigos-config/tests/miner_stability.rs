use serde_json::Value;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn write_runtime_status(path: &Path, revision: &str) {
    fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({
            "schema": "rigos.runtime-config-status/v1",
            "outcome": "ready",
            "revision": revision
        }))
        .unwrap(),
    )
    .unwrap();
}

fn run_observer(
    root: &Path,
    systemctl: &Path,
    journalctl: &Path,
    systemctl_fixture: &Path,
    journal_fixture: &Path,
) -> Value {
    let status = Command::new("python3")
        .arg(repo_path(
            "build/usb/includes.chroot/usr/lib/rigos/rigos-miner-health",
        ))
        .env("RIGOS_RUNTIME_PATH", root.join("run"))
        .env("RIGOS_BOOT_ID_PATH", root.join("boot-id"))
        .env("RIGOS_CURRENT_REVISION_PATH", root.join("state/current"))
        .env("RIGOS_PROC_ROOT", root.join("proc"))
        .env("RIGOS_SYSTEMCTL", systemctl)
        .env("RIGOS_JOURNALCTL", journalctl)
        .env("RIGOS_SYSTEMCTL_FIXTURE", systemctl_fixture)
        .env("RIGOS_JOURNAL_FIXTURE", journal_fixture)
        .status()
        .unwrap();
    assert!(status.success());
    serde_json::from_slice(&fs::read(root.join("run/miner-health-status.json")).unwrap()).unwrap()
}

#[test]
fn miner_health_distinguishes_ready_external_wait_degraded_blocked_and_unknown() {
    let root = std::env::temp_dir().join(format!("rigos-miner-health-{}", Uuid::new_v4()));
    fs::create_dir_all(root.join("run")).unwrap();
    fs::create_dir_all(root.join("state/revisions/r1")).unwrap();
    fs::create_dir_all(root.join("proc/123")).unwrap();
    symlink("revisions/r1", root.join("state/current")).unwrap();
    fs::write(root.join("boot-id"), "boot-test\n").unwrap();
    fs::write(root.join("proc/uptime"), "1000.0 0.0\n").unwrap();
    fs::write(root.join("proc/123/stat"), "123 (xmrig) S\n").unwrap();
    write_runtime_status(&root.join("run/runtime-config-status.json"), "r1");

    let systemctl_fixture = root.join("systemctl.txt");
    fs::write(
        &systemctl_fixture,
        concat!(
            "ActiveState=active\n",
            "SubState=running\n",
            "MainPID=123\n",
            "NRestarts=2\n",
            "Result=success\n",
            "ExecMainStatus=0\n",
            "ActiveEnterTimestampMonotonic=100000000\n"
        ),
    )
    .unwrap();
    let journal_fixture = root.join("journal.txt");
    let systemctl = root.join("systemctl");
    let journalctl = root.join("journalctl");
    let journalctl_fail = root.join("journalctl-fail");
    write_executable(
        &systemctl,
        "#!/bin/sh\ncat \"$RIGOS_SYSTEMCTL_FIXTURE\"\n",
    );
    write_executable(
        &journalctl,
        "#!/bin/sh\ncat \"$RIGOS_JOURNAL_FIXTURE\"\n",
    );
    write_executable(&journalctl_fail, "#!/bin/sh\nexit 1\n");

    fs::write(
        &journal_fixture,
        concat!(
            "miner    speed 10s/60s/15m 340.0 341.0 n/a H/s\n",
            "cpu accepted (7/0) diff 10000\n"
        ),
    )
    .unwrap();
    let ready = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
    );
    assert_eq!(ready["state"], "ready");
    assert_eq!(ready["unit"]["restart_count"], 2);
    assert_eq!(ready["evidence"]["accepted_shares"], 7);
    assert_eq!(ready["evidence"]["rejected_shares"], 0);
    assert_eq!(ready["remediation"], "observe_only");

    fs::write(&journal_fixture, "net connect error: connection refused\n").unwrap();
    let waiting = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
    );
    assert_eq!(waiting["state"], "waiting_external");
    assert_eq!(waiting["reason"], "pool_or_network_unavailable");

    fs::write(&journal_fixture, "").unwrap();
    let degraded = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
    );
    assert_eq!(degraded["state"], "degraded");
    assert_eq!(degraded["reason"], "no_recent_speed_evidence");

    let unknown = run_observer(
        &root,
        &systemctl,
        &journalctl_fail,
        &systemctl_fixture,
        &journal_fixture,
    );
    assert_eq!(unknown["state"], "unknown");
    assert_eq!(unknown["reason"], "journal_unavailable");
    assert_eq!(unknown["evidence"]["journal_available"], false);

    write_runtime_status(&root.join("run/runtime-config-status.json"), "r2");
    let blocked = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
    );
    assert_eq!(blocked["state"], "blocked");
    assert_eq!(blocked["reason"], "runtime_revision_mismatch");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn miner_restart_policy_is_bounded_and_observer_never_mutates_service() {
    let stability = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-miner.service.d/stability.conf",
    ))
    .unwrap();
    assert!(stability.contains("StartLimitIntervalSec=10min"));
    assert!(stability.contains("StartLimitBurst=5"));
    assert!(stability.contains("Restart=on-failure"));
    assert!(stability.contains("RestartSec=15s"));
    assert!(stability.contains("TimeoutStopSec=30s"));
    assert!(!stability.contains("Restart=always"));

    let observer = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/usr/lib/rigos/rigos-miner-health",
    ))
    .unwrap();
    assert!(observer.contains("\"remediation\": \"observe_only\""));
    assert!(observer.contains("MAX_JOURNAL_LINES = 500"));
    assert!(!observer.contains("systemctl restart"));
    assert!(!observer.contains("systemctl kill"));

    let service = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-miner-health.service",
    ))
    .unwrap();
    assert!(!service.contains("Wants=rigos-miner.service"));
    assert!(!service.contains("Requires=rigos-miner.service"));

    let timer = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-miner-health.timer",
    ))
    .unwrap();
    assert!(timer.contains("OnBootSec=2min"));
    assert!(timer.contains("OnUnitActiveSec=1min"));

    let hook = fs::read_to_string(repo_path("build/usb/hooks/010-rigos.chroot")).unwrap();
    assert!(hook.contains("rigos-miner-health.timer"));
}
