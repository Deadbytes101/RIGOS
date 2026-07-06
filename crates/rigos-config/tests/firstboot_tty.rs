use std::fs;
use std::path::PathBuf;

fn unit(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot/etc/systemd/system")
        .join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn firstboot_releases_tty1_to_getty_after_exit() {
    let unit = unit("rigos-firstboot.service");

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
    assert!(
        !unit.lines().any(|line| line == "TTYVHangup=yes"),
        "firstboot must not hang up the shared local console"
    );
}

#[test]
fn recovery_access_does_not_hang_up_the_following_firstboot_session() {
    let unit = unit("rigos-recovery-access.service");

    assert!(
        unit.lines()
            .any(|line| line.contains("Before=rigos-state-ready.service rigos-firstboot.service")),
        "recovery access must complete before firstboot"
    );
    assert!(
        !unit.lines().any(|line| line == "TTYVHangup=yes"),
        "recovery access must not hang up tty1 before firstboot starts"
    );
}
