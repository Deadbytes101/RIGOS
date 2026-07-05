use std::fs;
use std::path::PathBuf;

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)
}

#[test]
fn state_service_wires_debian_lsblk_compatibility() {
    let service_path = repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-state.service",
    );
    let wrapper_path = repo_path("build/usb/includes.chroot/usr/lib/rigos/lsblk-compat");
    let service = fs::read_to_string(&service_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", service_path.display()));
    let wrapper = fs::read_to_string(&wrapper_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", wrapper_path.display()));

    assert!(service.contains("RuntimeDirectory=rigos"));
    assert!(service.contains("/run/rigos/compat-bin/lsblk"));
    assert!(service.contains("PATH=/run/rigos/compat-bin:"));
    assert!(wrapper.contains("/usr/bin/lsblk"));
    assert!(wrapper.contains("sysfs_root / major_minor / \"partition\""));
    assert!(wrapper.contains("device[\"partn\"] = partition_number"));
    assert!(!wrapper.contains("startswith(\"/dev/sd\""));
}
