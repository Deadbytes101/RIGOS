use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf()
}

#[test]
fn alpha26_status_agent_is_built_in_but_opt_in() {
    let root = repository_root();
    let agent = root.join("build/usb/includes.chroot/usr/lib/rigos/rigos-status-agent");
    let operator = root.join("build/usb/includes.chroot/usr/local/bin/rig-status-agent");
    let service =
        root.join("build/usb/includes.chroot/etc/systemd/system/rigos-status-agent.service");
    let timer = root.join("build/usb/includes.chroot/etc/systemd/system/rigos-status-agent.timer");
    for path in [&agent, &operator, &service, &timer] {
        assert!(path.is_file(), "missing {}", path.display());
    }

    let hook = fs::read_to_string(root.join("build/usb/hooks/010-rigos.chroot")).unwrap();
    assert!(hook.contains("/usr/lib/rigos/rigos-status-agent"));
    assert!(hook.contains("/usr/local/bin/rig-status-agent"));
    assert!(hook.contains("systemctl disable rigos-status-agent.timer"));
    assert!(!hook.contains("systemctl enable rigos-status-agent.timer"));

    let service_text = fs::read_to_string(service).unwrap();
    assert!(service_text.contains("ConditionPathExists=/var/lib/rigos/status-agent/config.env"));
    assert!(service_text.contains("ConditionPathExists=/var/lib/rigos/status-agent/ingest.secret"));
    assert!(service_text.contains("SuccessExitStatus=75 76"));
    assert!(service_text.contains("ReadWritePaths=/var/lib/rigos/status-agent"));
    assert!(!service_text.contains("Requires=rigos-miner.service"));
    assert!(!service_text.contains("Before=rigos-miner.service"));

    let version = fs::read_to_string(root.join("build/usb/version.env")).unwrap();
    assert!(version.contains("RIGOS_PRODUCT_VERSION=0.0.4-alpha.26"));
    assert!(version.contains("RIGOS_BUILD_ORDINAL=26"));
}

#[test]
fn status_agent_has_no_baked_secret_or_private_mining_fields() {
    let root = repository_root();
    let agent =
        fs::read_to_string(root.join("build/usb/includes.chroot/usr/lib/rigos/rigos-status-agent"))
            .unwrap();
    assert!(agent.contains("COMPONENT_IDS"));
    assert!(agent.contains("rigos.status-observation/v1"));
    assert!(agent.contains("unexpected_failed_units"));
    assert!(agent.contains("return 75"));
    assert!(agent.contains("return 76"));

    for forbidden_path in [
        "build/usb/includes.chroot/var/lib/rigos/status-agent/ingest.secret",
        "build/usb/includes.chroot/var/lib/rigos/status-agent/source-id",
        "build/usb/includes.chroot/var/lib/rigos/status-agent/last-send.json",
    ] {
        assert!(
            !root.join(forbidden_path).exists(),
            "baked runtime state: {forbidden_path}"
        );
    }
}

#[test]
fn status_agent_python_contract_passes() {
    let root = repository_root();
    let compile = Command::new("python3")
        .arg("-m")
        .arg("py_compile")
        .arg(root.join("build/usb/includes.chroot/usr/lib/rigos/rigos-status-agent"))
        .arg(root.join("build/usb/includes.chroot/usr/local/bin/rig-status-agent"))
        .current_dir(&root)
        .status()
        .expect("python3 must compile status-agent entrypoints");
    assert!(compile.success());

    let status = Command::new("python3")
        .arg(root.join("scripts/test-status-agent.py"))
        .current_dir(&root)
        .status()
        .expect("python3 must run status-agent tests");
    assert!(status.success());
}
