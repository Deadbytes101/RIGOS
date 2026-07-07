use rigos_machine::MachineContext;
use rigos_miner::MinerBackend;
use rigos_xmrig::{ConfigParseState, XmrigBackend};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn explicit_public_config_wins_when_runtime_uses_xmrig_short_config_option() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rigos-explicit-config-{unique}"));
    let proc_root = root.join("proc");
    let pid_dir = proc_root.join("42");
    fs::create_dir_all(&pid_dir).unwrap();

    let private = root.join("private.json");
    let public = root.join("public.json");
    fs::write(
        &private,
        r#"{"algo":"rx/private","threads":99,"future":"PRIVATE_SENTINEL","http":{"enabled":false}}"#,
    )
    .unwrap();
    fs::write(
        &public,
        r#"{"algo":"rx/0","threads":2,"randomx":{"huge-pages":true},"pools":[{"url":"pool.test:1"}],"http":{"enabled":false}}"#,
    )
    .unwrap();

    fs::write(pid_dir.join("comm"), "xmrig\n").unwrap();
    let mut cmdline = Vec::new();
    cmdline.extend_from_slice(b"xmrig\0-c\0");
    cmdline.extend_from_slice(private.as_os_str().as_encoded_bytes());
    cmdline.push(0);
    fs::write(pid_dir.join("cmdline"), cmdline).unwrap();
    fs::write(
        pid_dir.join("status"),
        "Name:\txmrig\nUid:\t1000 1000 1000 1000\n",
    )
    .unwrap();
    fs::write(pid_dir.join("cgroup"), "0::/system.slice/rigos-miner.service\n").unwrap();
    fs::write(
        pid_dir.join("stat"),
        "42 (xmrig) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 100 0\n",
    )
    .unwrap();
    fs::write(proc_root.join("uptime"), "100.0 0.0\n").unwrap();

    let expected_path = public.to_string_lossy().into_owned();
    let backend = XmrigBackend {
        explicit_executable: None,
        explicit_config: Some(PathBuf::from(&public)),
        probe_version: false,
    };
    let result = backend.discover(&MachineContext {
        proc_root,
        sys_root: root.join("sys"),
    });
    let _ = fs::remove_dir_all(root);

    let snapshot = result.value.unwrap();
    assert!(snapshot.running);
    assert_eq!(snapshot.config.path, Some(expected_path));
    assert!(matches!(snapshot.config.parse_state, ConfigParseState::Valid));
    assert_eq!(snapshot.config.algorithm.as_deref(), Some("rx/0"));
    assert_eq!(snapshot.config.thread_hint, Some(2));
    assert_eq!(snapshot.config.huge_pages_requested, Some(true));
    assert_eq!(snapshot.config.pools, vec!["pool.test:1"]);
    assert!(!serde_json::to_string(&snapshot)
        .unwrap()
        .contains("PRIVATE_SENTINEL"));
}
