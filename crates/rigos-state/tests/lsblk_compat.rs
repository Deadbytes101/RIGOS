use std::fs;
use std::path::PathBuf;

use rigos_state::{LsblkDocument, boot_parent_disk};

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn state_service_wires_debian_lsblk_compatibility() {
    let service_path =
        repo_path("build/usb/includes.chroot/etc/systemd/system/rigos-state.service");
    let wrapper_path = repo_path("build/usb/includes.chroot/usr/lib/rigos/lsblk-compat");
    let tmpfiles_path = repo_path("build/usb/includes.chroot/usr/lib/tmpfiles.d/rigos.conf");
    let state_source_path = repo_path("crates/rigos-state/src/main.rs");
    let service = fs::read_to_string(&service_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", service_path.display()));
    let wrapper = fs::read_to_string(&wrapper_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", wrapper_path.display()));
    let state_source = fs::read_to_string(&state_source_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", state_source_path.display()));
    let tmpfiles = fs::read_to_string(&tmpfiles_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", tmpfiles_path.display()));

    assert!(!service.contains("RuntimeDirectory=rigos"));
    assert!(service.contains("systemd-tmpfiles --create /usr/lib/tmpfiles.d/rigos.conf"));
    assert_eq!(tmpfiles.trim(), "d /run/rigos 0755 root root -");
    assert!(!service.contains("ExecStartPre=/usr/bin/install"));
    assert!(!service.contains("Environment=PATH="));
    assert!(!service.contains("/run/rigos/compat-bin/lsblk"));
    assert!(wrapper.contains("/usr/bin/lsblk"));
    assert!(wrapper.contains("sysfs_root / major_minor / \"partition\""));
    assert!(wrapper.contains("device[\"partn\"] = partition_number"));
    assert!(!wrapper.contains("startswith(\"/dev/sd\""));
    assert!(state_source.contains("run(\n        \"/usr/bin/python3\""));
    assert!(state_source.contains("\"/usr/lib/rigos/lsblk-compat\""));
    assert!(state_source.contains("\"--tree\""));
    assert!(!state_source.contains("/run/rigos/compat-bin/lsblk"));
    assert!(!state_source.contains("run(\n        \"lsblk\""));
}

#[test]
fn compat_tree_output_resolves_boot_parent_by_block_identity() {
    let fixture_path = repo_path("crates/rigos-state/tests/fixtures/lsblk-tree.json");
    let fixture = fs::read_to_string(&fixture_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", fixture_path.display()));
    let observed: LsblkDocument = serde_json::from_str(&fixture)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", fixture_path.display()));

    let disk = boot_parent_disk(&observed, "8:18").expect("boot parent must be resolved");
    assert_eq!(disk.path, "/dev/sdb");
    assert_eq!(
        disk.children
            .iter()
            .map(|child| child.path.as_str())
            .collect::<Vec<_>>(),
        vec!["/dev/sdb1", "/dev/sdb2", "/dev/sdb3", "/dev/sdb4"]
    );
    assert!(boot_parent_disk(&observed, "8:99").is_none());
}
