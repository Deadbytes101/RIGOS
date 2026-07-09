use std::fs;
use std::path::PathBuf;

fn preview_script() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts/preview-firstboot-theme.sh");
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn preview_covers_every_firstboot_dialog_class() {
    let script = preview_script();

    for required in [
        "preview_menu",
        "preview_input",
        "preview_confirm",
        "preview_message",
        "menu|input|confirm|message|all",
    ] {
        assert!(
            script.contains(required),
            "preview harness is missing required mode marker: {required}"
        );
    }

    for required in ["--menu", "--inputbox", "--yesno", "--msgbox"] {
        assert!(
            script.contains(required),
            "preview harness is missing dialog class: {required}"
        );
    }

    assert!(
        !script.contains("//"),
        "preview titles must not use synthetic slash separators"
    );
}

#[test]
fn preview_is_read_only_and_does_not_touch_appliance_state() {
    let script = preview_script();

    for forbidden in [
        "/var/lib/rigos",
        "systemctl",
        "rigos-config",
        "rigos-miner",
        "rigos-randomx-msr",
        "mount ",
        "umount ",
        "rm -rf",
        "sudo ",
    ] {
        assert!(
            !script.contains(forbidden),
            "preview harness contains forbidden mutation surface: {forbidden}"
        );
    }

    assert!(script.contains("RIGOS_WHIPTAIL_REAL=/usr/bin/whiptail"));
    assert!(script.contains("RIGOS_THEME_PREVIEW_EXIT="));
}
