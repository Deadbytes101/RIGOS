use std::fs;
use std::path::PathBuf;

fn unit(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot/etc/systemd/system")
        .join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn appliance_file(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot")
        .join(path)
}

#[test]
fn firstboot_releases_tty1_to_getty_after_exit() {
    let unit = unit("rigos-firstboot.service");

    assert!(
        unit.lines().any(|line| line == "Before=getty@tty1.service"),
        "firstboot must finish before tty1 getty starts"
    );
    assert!(
        unit.lines()
            .any(|line| line == "ExecStart=/usr/local/sbin/rigos-firstboot-seeded"),
        "firstboot must launch the offline identity seed resolver"
    );
    assert!(
        appliance_file("usr/local/sbin/rigos-firstboot-seeded").is_file(),
        "seeded firstboot launcher is missing"
    );
    assert!(
        appliance_file("usr/lib/rigos/rigos-identity-seed").is_file(),
        "identity seed verifier is missing"
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

#[test]
fn state_ready_stays_out_of_the_local_fs_transaction() {
    let unit = unit("rigos-state-ready.service");

    assert!(!unit.lines().any(|line| line == "DefaultDependencies=no"));
    assert!(
        !unit
            .lines()
            .any(|line| line.contains("Before=local-fs.target"))
    );
    assert!(
        unit.lines()
            .any(|line| line == "WantedBy=multi-user.target")
    );
    assert!(
        unit.lines()
            .any(|line| line == "After=rigos-state.service rigos-recovery-access.service")
    );
    assert!(
        unit.lines()
            .any(|line| line == "Requires=rigos-state.service")
    );
    assert!(unit.lines().any(|line| {
        line == "Before=rigos-profile-apply.service rigos-firstboot.service rigos-hugepages.service rigos-miner.service"
    }));
}
