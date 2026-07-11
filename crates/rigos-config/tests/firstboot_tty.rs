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
fn firstboot_remains_visible_when_state_readiness_fails() {
    let unit = unit("rigos-firstboot.service");
    let after = directive_tokens(&unit, "After=");
    let wants = directive_tokens(&unit, "Wants=");
    let requires = directive_tokens(&unit, "Requires=");

    for ordered in [
        "rigos-state.service",
        "rigos-state-ready.service",
        "rigos-profile-apply.service",
    ] {
        assert!(
            after.contains(ordered),
            "firstboot must start after {ordered} finishes, including failure"
        );
    }

    assert!(
        wants.contains("rigos-state-ready.service"),
        "firstboot must request state verification without depending on success"
    );
    assert!(
        !requires.contains("rigos-state-ready.service"),
        "state verification failure must not suppress firstboot diagnostics"
    );
    assert!(
        !unit.contains("network-online.target"),
        "local firstboot must remain available without network connectivity"
    );
    assert!(
        unit.lines().any(|line| line == "StandardInput=tty-force"),
        "firstboot must acquire tty1 for the local setup flow"
    );
    assert!(
        unit.lines().any(|line| line == "TTYPath=/dev/tty1"),
        "firstboot must remain bound to tty1"
    );
    assert!(
        unit.lines().any(|line| line == "StandardOutput=tty"),
        "firstboot UI must keep stdout attached to tty1"
    );
    assert!(
        unit.lines().any(|line| line == "StandardError=journal"),
        "firstboot diagnostics must go to the journal instead of corrupting tty1"
    );
    assert!(
        !unit.lines().any(|line| line == "StandardError=tty"),
        "firstboot stderr must not write diagnostics over the local UI"
    );
}

#[test]
fn recovery_access_does_not_hang_up_the_following_firstboot_session() {
    let unit = unit("rigos-recovery-access.service");
    let recovery = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/usb/includes.chroot/usr/local/sbin/rigos-recovery-access"),
    )
    .expect("read recovery access helper");
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
    assert!(
        unit.lines().any(|line| line == "StandardError=journal"),
        "recovery diagnostics must go to the journal instead of corrupting tty1"
    );
    assert!(
        unit.lines().any(|line| line == "TTYVTDisallocate=yes"),
        "recovery access must clear the tty before handing off to firstboot"
    );
    assert!(
        recovery.contains("CONSOLE = Path(os.environ.get(\"RIGOS_CONSOLE\", \"/dev/tty1\"))"),
        "recovery prompts must use an explicit console path"
    );
    assert!(
        recovery.contains("stdin=console")
            && recovery.contains("stdout=console")
            && recovery.contains("stderr=console"),
        "recovery UI children must render on the console even when service stderr is journaled"
    );
}

#[test]
fn bootloader_uses_quiet_productized_console_entries() {
    let builder = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/build-usb-image.sh"),
    )
    .expect("read USB build script");
    let syslinux = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/usb/bootloaders/syslinux_common/live.cfg.in"),
    )
    .expect("read recovery bootloader template");
    let theme = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/usb/bootloaders/grub-pc/live-theme/theme.txt"),
    )
    .expect("read recovery GRUB theme");

    for required in ["quiet", "loglevel=3", "systemd.show_status=false"] {
        assert!(
            builder.contains(required),
            "USB kernel command line must include {required}"
        );
    }
    assert!(
        builder.contains("menuentry 'RIGOS ${RIGOS_IMAGE_VERSION}  SAFE MODE'"),
        "safe mode GRUB label must be productized"
    );
    assert!(
        builder.contains("menuentry 'RIGOS ${RIGOS_IMAGE_VERSION}  FALLBACK SLOT B'"),
        "fallback GRUB label must be productized"
    );
    assert!(
        !builder.contains("-- safe mode") && !builder.contains("RIGOS ROOT_B fallback"),
        "USB GRUB labels must not expose developer wording"
    );
    assert!(
        syslinux.contains("menu label ^RIGOS Recovery")
            && syslinux.contains("menu label RIGOS Recovery (^Safe Mode)"),
        "recovery boot menu labels must be productized"
    );
    assert!(
        theme.contains("title-text: \"RIGOS Recovery\""),
        "recovery GRUB theme must use a productized title"
    );
}

#[test]
fn primary_usb_grub_uses_rigos_theme_with_text_fallback() {
    let builder = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/build-usb-image.sh"),
    )
    .expect("read USB build script");
    let theme = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/usb/grub-theme/rigos/theme.txt"),
    )
    .expect("read primary GRUB theme");

    for required in [
        "theme_source=\"$source_root/build/usb/grub-theme/rigos\"",
        "[[ -f \"$theme_source/theme.txt\" ]] || die \"RIGOS GRUB theme definition is missing\"",
        "cp -a \"$theme_source/.\" \"$theme_dest/\"",
        "find \"$theme_dest\" -type d -exec chmod 0755 {} +",
        "find \"$theme_dest\" -type f -exec chmod 0644 {} +",
        "set theme=/boot/grub/themes/rigos/theme.txt",
        "set menu_color_highlight=white/blue",
        "terminal_output gfxterm",
        "insmod png",
    ] {
        assert!(
            builder.contains(required),
            "primary USB GRUB theme wiring is missing: {required}"
        );
    }

    assert!(
        !builder.contains("install -m 0644 build/usb/grub-theme/rigos/*"),
        "primary USB GRUB theme installation must not use caller cwd or flat globs"
    );

    for required in [
        "title-text: \"RIGOS\"",
        "USB COMPUTE APPLIANCE",
        "0.0.4-alpha.17",
        "ENTER BOOT",
        "selected_item_pixmap_style = \"select_*.png\"",
    ] {
        assert!(
            theme.contains(required),
            "primary USB GRUB theme is missing: {required}"
        );
    }

    assert!(
        !theme.contains("select_*.txt"),
        "primary USB GRUB theme must not reference text pseudo-images"
    );

    let theme_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../build/usb/grub-theme/rigos");

    for slice in ["c", "n", "ne", "e", "se", "s", "sw", "w", "nw"] {
        let asset = theme_dir.join(format!("select_{slice}.png"));

        assert!(
            asset.is_file(),
            "primary USB GRUB selected-item slice is missing: {}",
            asset.display()
        );

        let bytes = fs::read(&asset).expect("read primary GRUB PNG slice");

        assert!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "primary USB GRUB selected-item slice is not PNG: {}",
            asset.display()
        );
    }
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
