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

fn python_executable() -> String {
    if let Ok(value) = std::env::var("RIGOS_TEST_PYTHON") {
        let value = value.trim();
        if !value.is_empty() {
            return value.to_owned();
        }
    }

    for candidate in ["python3", "python"] {
        let available = Command::new(candidate)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if available {
            return candidate.to_owned();
        }
    }

    panic!("Python 3 interpreter not found; set RIGOS_TEST_PYTHON to its executable path");
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
fn alpha26_has_a_real_time_synchronization_authority() {
    let root = repository_root();
    let packages = fs::read_to_string(root.join("build/usb/package-lists/rigos.list.chroot"))
        .expect("read package list");
    assert!(
        packages
            .lines()
            .any(|line| line.trim() == "systemd-timesyncd"),
        "systemd-timesyncd must be in the immutable image"
    );

    let hook = fs::read_to_string(root.join("build/usb/hooks/010-rigos.chroot")).unwrap();
    assert!(hook.contains("systemd-timesyncd.service"));
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
    assert!(agent.contains("root_filesystem_authority"));
    assert!(agent.contains("kernel_fault_counts"));
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
    let python = python_executable();

    let compile = Command::new(&python)
        .arg("-m")
        .arg("py_compile")
        .arg(root.join("build/usb/includes.chroot/usr/lib/rigos/rigos-status-agent"))
        .arg(root.join("build/usb/includes.chroot/usr/local/bin/rig-status-agent"))
        .current_dir(&root)
        .status()
        .expect("Python must compile status-agent entrypoints");
    assert!(compile.success());

    let status = Command::new(&python)
        .arg(root.join("scripts/test-status-agent.py"))
        .current_dir(&root)
        .status()
        .expect("Python must run status-agent tests");
    assert!(status.success());
}
