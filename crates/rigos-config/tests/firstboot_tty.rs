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

fn system_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot/etc/systemd/system")
        .join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn directive_tokens<'a>(unit: &'a str, prefix: &str) -> BTreeSet<&'a str> {
    unit.lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .flat_map(str::split_whitespace)
        .collect()
}

#[derive(Clone, Copy)]
enum BootMode {
    Normal,
    Utility,
}

#[derive(Clone, Copy)]
enum Configuration {
    Missing,
    Present,
}

#[derive(Debug)]
struct Transaction {
    firstboot_requested: bool,
    firstboot_condition_passed: bool,
    firstboot_execcondition_passed: bool,
    utility_requested: bool,
    utility_condition_passed: bool,
    getty_waits_for_firstboot: bool,
    getty_waits_for_utility: bool,
}

fn condition_passes(unit: &str, mode: BootMode) -> bool {
    let conditions = directive_tokens(unit, "ConditionKernelCommandLine=");
    if conditions.contains("rigos.utility=1") {
        return matches!(mode, BootMode::Utility);
    }
    if conditions.contains("!rigos.utility=1") {
        return matches!(mode, BootMode::Normal);
    }
    true
}

fn simulate_initial_tty1_transaction(mode: BootMode, configuration: Configuration) -> Transaction {
    let firstboot = unit("rigos-firstboot.service");
    let utility = unit("rigos-boot-utility.service");
    let getty_dropin = system_file("getty@tty1.service.d/rigos-firstboot.conf");

    let firstboot_enabled = true;
    let utility_enabled = true;
    let mut firstboot_requested = firstboot_enabled
        || directive_tokens(&getty_dropin, "Wants=").contains("rigos-firstboot.service");
    let utility_requested = utility_enabled;

    if firstboot_requested
        && utility_requested
        && directive_tokens(&utility, "Conflicts=").contains("rigos-firstboot.service")
    {
        firstboot_requested = false;
    }

    let firstboot_condition_passed = firstboot_requested && condition_passes(&firstboot, mode);
    let firstboot_execcondition_passed =
        firstboot_condition_passed && matches!(configuration, Configuration::Missing);
    let utility_condition_passed = utility_requested && condition_passes(&utility, mode);

    Transaction {
        firstboot_requested,
        firstboot_condition_passed,
        firstboot_execcondition_passed,
        utility_requested,
        utility_condition_passed,
        getty_waits_for_firstboot: directive_tokens(&firstboot, "Before=")
            .contains("getty@tty1.service")
            && directive_tokens(&getty_dropin, "After=").contains("rigos-firstboot.service"),
        getty_waits_for_utility: directive_tokens(&utility, "Before=")
            .contains("getty@tty1.service"),
    }
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

    for ordered in ["rigos-state.service", "rigos-state-ready.service"] {
        assert!(
            after.contains(ordered),
            "firstboot must start after {ordered} finishes, including failure"
        );
    }
    assert!(
        !after.contains("rigos-profile-apply.service"),
        "firstboot must not wait on profile apply before the initial interactive commit"
    );

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
fn tty1_getty_transaction_queues_firstboot_before_login_prompt() {
    let firstboot = unit("rigos-firstboot.service");
    let getty_dropin = system_file("getty@tty1.service.d/rigos-firstboot.conf");

    assert!(
        directive_tokens(&firstboot, "Before=").contains("getty@tty1.service"),
        "firstboot must still order before tty1 getty"
    );
    assert!(
        directive_tokens(&getty_dropin, "Wants=").contains("rigos-firstboot.service"),
        "tty1 getty must pull firstboot into the boot transaction"
    );
    assert!(
        directive_tokens(&getty_dropin, "After=").contains("rigos-firstboot.service"),
        "tty1 getty must wait for firstboot to finish or skip"
    );
}

#[test]
fn profile_apply_uses_complete_machine_profile_command() {
    let profile = unit("rigos-profile-apply.service");
    assert!(
        profile
            .lines()
            .any(|line| line == "ExecStart=/usr/lib/rigos/rigos-config profile"),
        "profile apply must apply hostname and timezone together"
    );
    assert!(
        !profile.contains("rigos-config timezone"),
        "profile apply must not use the legacy timezone-only path"
    );
    assert!(
        !directive_tokens(&profile, "Before=").contains("rigos-firstboot.service"),
        "profile apply must not be required before interactive firstboot is queued"
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
        builder.contains("menuentry 'RIGOS ${RIGOS_IMAGE_VERSION}  UTILITY MODE'")
            && builder.contains("rigos.utility=1"),
        "utility GRUB entry must boot the local console utility mode"
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
fn utility_boot_mode_owns_tty_without_competing_with_firstboot() {
    let firstboot = unit("rigos-firstboot.service");
    let utility = unit("rigos-boot-utility.service");

    assert!(
        firstboot
            .lines()
            .any(|line| line == "ConditionKernelCommandLine=!rigos.utility=1"),
        "firstboot must be disabled on utility boot mode"
    );
    assert!(
        utility
            .lines()
            .any(|line| line == "ConditionKernelCommandLine=rigos.utility=1"),
        "utility mode must be selected by an explicit kernel argument"
    );
    assert!(
        !directive_tokens(&utility, "Conflicts=").contains("rigos-firstboot.service"),
        "utility mode must not conflict firstboot out of the initial transaction before conditions run"
    );
    assert!(
        !directive_tokens(&firstboot, "Conflicts=").contains("rigos-boot-utility.service"),
        "firstboot must not use a reciprocal conflict against utility mode"
    );
    assert!(
        utility
            .lines()
            .any(|line| line == "ExecStart=/usr/local/sbin/rigos-utility"),
        "utility mode must launch the local utility"
    );
    assert!(
        utility
            .lines()
            .any(|line| line == "StandardInput=tty-force")
            && utility.lines().any(|line| line == "TTYPath=/dev/tty1"),
        "utility mode must own tty1"
    );
}

#[test]
fn initial_boot_transaction_keeps_mode_selection_in_conditions_not_conflicts() {
    let normal_no_config =
        simulate_initial_tty1_transaction(BootMode::Normal, Configuration::Missing);
    assert!(
        normal_no_config.firstboot_requested,
        "normal unconfigured boot must queue firstboot"
    );
    assert!(normal_no_config.firstboot_condition_passed);
    assert!(normal_no_config.firstboot_execcondition_passed);
    assert!(normal_no_config.utility_requested);
    assert!(!normal_no_config.utility_condition_passed);
    assert!(normal_no_config.getty_waits_for_firstboot);

    let utility_boot = simulate_initial_tty1_transaction(BootMode::Utility, Configuration::Missing);
    assert!(utility_boot.utility_requested);
    assert!(utility_boot.utility_condition_passed);
    assert!(utility_boot.firstboot_requested);
    assert!(!utility_boot.firstboot_condition_passed);
    assert!(utility_boot.getty_waits_for_utility);

    let configured_normal =
        simulate_initial_tty1_transaction(BootMode::Normal, Configuration::Present);
    assert!(configured_normal.firstboot_requested);
    assert!(configured_normal.firstboot_condition_passed);
    assert!(
        !configured_normal.firstboot_execcondition_passed,
        "configured normal boot must skip firstboot through ExecCondition, not through transaction loss"
    );
    assert!(!configured_normal.utility_condition_passed);
    assert!(configured_normal.getty_waits_for_firstboot);
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
        "0.0.4-alpha.22",
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
