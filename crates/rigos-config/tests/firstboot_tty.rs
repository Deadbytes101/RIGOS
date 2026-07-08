use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn unit(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot/etc/systemd/system")
        .join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn directive_tokens<'a>(unit: &'a str, prefix: &str) -> BTreeSet<&'a str> {
    unit.lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .flat_map(str::split_whitespace)
        .collect()
}

#[test]
fn firstboot_releases_tty1_to_getty_after_exit() {
    let unit = unit("rigos-firstboot.service");

    assert!(
        directive_tokens(&unit, "Before=").contains("getty@tty1.service"),
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
    let before = directive_tokens(&unit, "Before=");

    for required in ["rigos-state-ready.service", "rigos-firstboot.service"] {
        assert!(
            before.contains(required),
            "recovery access must complete before {required}"
        );
    }
    assert!(
        !unit.lines().any(|line| line == "TTYVHangup=yes"),
        "recovery access must not hang up tty1 before firstboot starts"
    );
}

#[test]
fn state_ready_stays_out_of_the_local_fs_transaction() {
    let unit = unit("rigos-state-ready.service");
    let after = directive_tokens(&unit, "After=");
    let requires = directive_tokens(&unit, "Requires=");
    let before = directive_tokens(&unit, "Before=");

    assert!(!unit.lines().any(|line| line == "DefaultDependencies=no"));
    assert!(
        !before.contains("local-fs.target"),
        "state readiness must stay out of the local-fs transaction"
    );
    assert!(
        directive_tokens(&unit, "WantedBy=").contains("multi-user.target"),
        "state readiness must be installed under multi-user.target"
    );

    for required in ["rigos-state.service", "rigos-recovery-access.service"] {
        assert!(
            after.contains(required),
            "state readiness must start after {required}"
        );
    }
    assert!(
        requires.contains("rigos-state.service"),
        "state readiness must require persistent state"
    );

    for required in [
        "rigos-ssh-hostkeys.service",
        "rigos-profile-apply.service",
        "rigos-firstboot.service",
        "rigos-hugepages.service",
        "rigos-miner.service",
    ] {
        assert!(
            before.contains(required),
            "state readiness must complete before {required}"
        );
    }
}
