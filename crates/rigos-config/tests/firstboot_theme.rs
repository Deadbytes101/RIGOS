use std::fs;
use std::path::PathBuf;

const WRAPPER_PATH: &str = "build/usb/includes.chroot/usr/lib/rigos/rigos-firstboot-whiptail";

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn repo_file(path: &str) -> String {
    let path = repo_path(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn firstboot_uses_the_dedicated_console_theme_wrapper() {
    let drop_in = repo_file(
        "build/usb/includes.chroot/etc/systemd/system/rigos-firstboot.service.d/2009-console-theme.conf",
    );

    assert!(
        drop_in.contains("Environment=RIGOS_WHIPTAIL=/usr/lib/rigos/rigos-firstboot-whiptail"),
        "firstboot must select the dedicated presentation wrapper"
    );
    assert!(
        drop_in.contains("RIGOS SETUP UTILITY   LOCAL NODE CONFIGURATION"),
        "firstboot must publish the static non-secret backtitle"
    );
    assert!(
        !drop_in.contains("//"),
        "setup utility titles must not use synthetic slash separators"
    );
}

#[test]
fn firstboot_wrapper_is_pinned_to_lf_in_git() {
    let attributes = repo_file(".gitattributes");
    let required = "build/usb/includes.chroot/usr/lib/rigos/rigos-firstboot-* text eol=lf";

    assert!(
        attributes.lines().any(|line| line == required),
        "firstboot runtime wrappers must remain LF on Windows and WSL checkouts"
    );
}

#[test]
fn firstboot_theme_is_ascii_and_preserves_whiptail_as_the_ui_engine() {
    let wrapper = repo_file(WRAPPER_PATH);

    assert!(wrapper.is_ascii(), "console theme must remain ASCII-only");
    assert!(wrapper.contains("NEWT_COLORS="));
    assert!(wrapper.contains("root=white,blue"));
    assert!(wrapper.contains("window=black,lightgray"));
    assert!(wrapper.contains("actlistbox=white,blue"));
    assert!(wrapper.contains("--backtitle"));
    assert!(wrapper.contains("--ok-button SELECT --cancel-button BACK"));
    assert!(wrapper.contains("--yes-button APPLY --no-button BACK"));
    assert!(wrapper.contains("RIGOS_WHIPTAIL_REAL:-/usr/bin/whiptail"));
    assert!(wrapper.contains("exec \"$whiptail_real\""));
    assert!(wrapper.contains("backend is not executable"));
    assert!(!wrapper.contains("//"));
    assert!(
        !wrapper.contains("xterm")
            && !wrapper.contains("Xorg")
            && !wrapper.contains("wayland")
            && !wrapper.contains("electron"),
        "theme must not introduce a graphical runtime"
    );
}

#[test]
fn image_hook_installs_the_theme_wrapper_as_executable() {
    let hook = repo_file("build/usb/hooks/010-rigos.chroot");

    assert!(
        hook.contains("/usr/lib/rigos/rigos-firstboot-whiptail"),
        "image construction must install the wrapper as executable"
    );
}
