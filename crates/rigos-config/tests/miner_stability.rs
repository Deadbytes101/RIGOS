#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use uuid::Uuid;

const API_TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

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

fn api_port_and_server(summary: Option<Value>) -> (u16, Option<thread::JoinHandle<()>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    let Some(summary) = summary else {
        drop(listener);
        return (port, None);
    };

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            assert!(request.len() <= 8192, "observer HTTP request is oversized");
        }

        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with("GET /2/summary HTTP/1.1\r\n"));
        assert!(request.contains(&format!("Authorization: Bearer {API_TOKEN}\r\n")));

        let body = serde_json::to_vec(&summary).unwrap();
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        stream.flush().unwrap();
    });

    (port, Some(server))
}

fn run_observer(
    root: &Path,
    systemctl: &Path,
    journalctl: &Path,
    systemctl_fixture: &Path,
    journal_fixture: &Path,
    api_summary: Option<Value>,
) -> Value {
    let (api_port, server) = api_port_and_server(api_summary);
    let status = Command::new("python3")
        .arg(repo_path(
            "build/usb/includes.chroot/usr/lib/rigos/rigos-miner-health",
        ))
        .env("RIGOS_RUNTIME_PATH", root.join("run"))
        .env(
            "RIGOS_MINER_HEALTH_STATE_DIR",
            root.join("state/system/miner-health"),
        )
        .env("RIGOS_BOOT_ID_PATH", root.join("boot-id"))
        .env("RIGOS_CURRENT_REVISION_PATH", root.join("state/current"))
        .env("RIGOS_PROC_ROOT", root.join("proc"))
        .env("RIGOS_SYSTEMCTL", systemctl)
        .env("RIGOS_JOURNALCTL", journalctl)
        .env("RIGOS_SYSTEMCTL_FIXTURE", systemctl_fixture)
        .env("RIGOS_JOURNAL_FIXTURE", journal_fixture)
        .env(
            "RIGOS_XMRIG_API_TOKEN_PATH",
            root.join("run/xmrig-api-token"),
        )
        .env("RIGOS_XMRIG_API_HOST", "127.0.0.1")
        .env("RIGOS_XMRIG_API_PORT", api_port.to_string())
        .env("RIGOS_XMRIG_API_TIMEOUT_SECONDS", "0.5")
        .env("RIGOS_MINER_HEALTH_TEST_MODE", "1")
        .status()
        .unwrap();

    if let Some(server) = server {
        server.join().unwrap();
    }
    assert!(status.success());
    serde_json::from_slice(&fs::read(root.join("run/miner-health-status.json")).unwrap()).unwrap()
}

fn ready_summary() -> Value {
    serde_json::json!({
        "uptime": 600,
        "algo": "rx/0",
        "hashrate": {
            "total": [340.0, 341.0, 339.0],
            "highest": 342.0
        },
        "connection": {
            "pool": "pool.test:1",
            "ip": "127.0.0.1",
            "uptime": 590,
            "accepted": 7,
            "rejected": 0,
            "failures": 0,
            "ping": 20
        },
        "hugepages": [4, 4]
    })
}

fn disconnected_summary() -> Value {
    serde_json::json!({
        "uptime": 600,
        "algo": "rx/0",
        "hashrate": {
            "total": [0.0, 341.0, 339.0],
            "highest": 342.0
        },
        "connection": {
            "accepted": 7,
            "rejected": 0,
            "failures": 1
        },
        "hugepages": [4, 4]
    })
}

fn missing_current_hashrate_summary() -> Value {
    serde_json::json!({
        "uptime": 600,
        "algo": "rx/0",
        "hashrate": {
            "total": [null, 341.0, 339.0],
            "highest": 342.0
        },
        "connection": {
            "pool": "pool.test:1",
            "ip": "127.0.0.1",
            "uptime": 590,
            "accepted": 7,
            "rejected": 0,
            "failures": 0,
            "ping": 20
        },
        "hugepages": [4, 4]
    })
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
    fs::write(root.join("run/xmrig-api-token"), format!("{API_TOKEN}\n")).unwrap();
    fs::set_permissions(
        root.join("run/xmrig-api-token"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
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
    write_executable(&systemctl, "#!/bin/sh\ncat \"$RIGOS_SYSTEMCTL_FIXTURE\"\n");
    write_executable(&journalctl, "#!/bin/sh\ncat \"$RIGOS_JOURNAL_FIXTURE\"\n");
    write_executable(&journalctl_fail, "#!/bin/sh\nexit 1\n");

    fs::write(
        &journal_fixture,
        "net connect error: stale journal evidence\n",
    )
    .unwrap();
    let ready = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
        Some(ready_summary()),
    );
    assert_eq!(ready["state"], "ready");
    assert_eq!(ready["reason"], Value::Null);
    assert_eq!(ready["unit"]["restart_count"], 2);
    assert_eq!(ready["evidence"]["source"], "xmrig_http_api");
    assert_eq!(ready["evidence"]["accepted_shares"], 7);
    assert_eq!(ready["evidence"]["rejected_shares"], 0);
    assert_eq!(ready["remediation"]["action"], "none");

    fs::write(
        &journal_fixture,
        "miner    speed 10s/60s/15m 340.0 341.0 339.0 H/s\n",
    )
    .unwrap();
    let waiting = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
        Some(disconnected_summary()),
    );
    assert_eq!(waiting["state"], "waiting_external");
    assert_eq!(waiting["reason"], "pool_or_network_unavailable");
    assert_eq!(waiting["evidence"]["source"], "xmrig_http_api");

    let degraded = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
        Some(missing_current_hashrate_summary()),
    );
    assert_eq!(degraded["state"], "degraded");
    assert_eq!(degraded["reason"], "current_hashrate_unavailable");
    assert_eq!(degraded["evidence"]["source"], "xmrig_http_api");

    fs::write(&journal_fixture, "").unwrap();
    let unknown = run_observer(
        &root,
        &systemctl,
        &journalctl_fail,
        &systemctl_fixture,
        &journal_fixture,
        None,
    );
    assert_eq!(unknown["state"], "unknown");
    assert_eq!(unknown["reason"], "api_unavailable");
    assert_eq!(unknown["evidence"]["journal_available"], false);

    write_runtime_status(&root.join("run/runtime-config-status.json"), "r2");
    let blocked = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
        Some(ready_summary()),
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
    assert!(observer.contains("RESTART_BUDGET_MAX"));
    assert!(observer.contains("RESTART_COOLDOWN_SECONDS"));
    assert!(observer.contains("\"restart_budget_exhausted\""));
    assert!(observer.contains("MAX_JOURNAL_LINES = 500"));
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

#[test]
fn miner_supervisor_restarts_only_after_persistent_actionable_fault() {
    let root = std::env::temp_dir().join(format!("rigos-miner-supervisor-{}", Uuid::new_v4()));
    fs::create_dir_all(root.join("run")).unwrap();
    fs::create_dir_all(root.join("state/revisions/r1")).unwrap();
    fs::create_dir_all(root.join("proc/123")).unwrap();
    symlink("revisions/r1", root.join("state/current")).unwrap();
    fs::write(root.join("boot-id"), "boot-test\n").unwrap();
    fs::write(root.join("proc/uptime"), "1000.0 0.0\n").unwrap();
    fs::write(root.join("proc/123/stat"), "123 (xmrig) S\n").unwrap();
    fs::write(root.join("run/xmrig-api-token"), format!("{API_TOKEN}\n")).unwrap();
    fs::set_permissions(
        root.join("run/xmrig-api-token"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    write_runtime_status(&root.join("run/runtime-config-status.json"), "r1");

    let systemctl_fixture = root.join("systemctl.txt");
    fs::write(
        &systemctl_fixture,
        concat!(
            "ActiveState=active\n",
            "SubState=running\n",
            "MainPID=123\n",
            "NRestarts=0\n",
            "Result=success\n",
            "ExecMainStatus=0\n",
            "ActiveEnterTimestampMonotonic=100000000\n"
        ),
    )
    .unwrap();
    let restart_log = root.join("restart.log");
    let systemctl = root.join("systemctl");
    write_executable(
        &systemctl,
        &format!(
            "#!/bin/sh\nif [ \"$1\" = restart ]; then echo restart >> '{}'; exit 0; fi\ncat \"$RIGOS_SYSTEMCTL_FIXTURE\"\n",
            restart_log.display()
        ),
    );
    let journal_fixture = root.join("journal.txt");
    fs::write(&journal_fixture, "miner    speed stale\n").unwrap();
    let journalctl = root.join("journalctl");
    write_executable(&journalctl, "#!/bin/sh\ncat \"$RIGOS_JOURNAL_FIXTURE\"\n");

    let first = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
        Some(missing_current_hashrate_summary()),
    );
    assert_eq!(first["state"], "degraded");
    assert_eq!(first["remediation"]["action"], "observe");
    assert!(!restart_log.exists());

    let second = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
        Some(missing_current_hashrate_summary()),
    );
    assert_eq!(second["state"], "degraded");
    assert_eq!(second["remediation"]["action"], "restart_miner");
    assert_eq!(fs::read_to_string(&restart_log).unwrap().lines().count(), 1);

    let waiting = run_observer(
        &root,
        &systemctl,
        &journalctl,
        &systemctl_fixture,
        &journal_fixture,
        Some(disconnected_summary()),
    );
    assert_eq!(waiting["state"], "waiting_external");
    assert_eq!(waiting["remediation"]["action"], "none");

    let _ = fs::remove_dir_all(root);
}
