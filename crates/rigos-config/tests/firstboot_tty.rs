use std::fs;
use std::path::PathBuf;

fn firstboot_unit() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot/etc/systemd/system/rigos-firstboot.service");
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn firstboot_releases_tty1_to_getty_after_exit() {
    let unit = firstboot_unit();

    assert!(
        unit.lines().any(|line| line == "Before=getty@tty1.service"),
        "firstboot must finish before tty1 getty starts"
    );
    assert!(
        !unit
            .lines()
            .any(|line| line == "Conflicts=getty@tty1.service"),
        "firstboot must not permanently block tty1 getty"
    );
    assert!(
        !unit.lines().any(|line| line == "RemainAfterExit=yes"),
        "firstboot must release tty1 after the process exits"
    );
}
