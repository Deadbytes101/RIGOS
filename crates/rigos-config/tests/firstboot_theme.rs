use std::fs;
use std::path::PathBuf;

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
        drop_in.contains("RIGOS SYSTEM CONFIGURATION // LOCAL NODE SETUP // OFFLINE AUTHORITY"),
        "firstboot must publish the static non-secret backtitle"
    );
}

#[test]
fn firstboot_wrapper_is_pinned_to_lf_in_git() {
    let attributes = repo_file(".gitattributes");

    assert!(
        attributes
            .lines()
            .any(|line| line == "build/usb/includes.chroot/usr/lib/rigos/rigos-firstboot-* text eol=lf"),
        "firstboot runtime wrappers must remain LF on Windows and WSL checkouts"
    );
}

#[test]
fn firstboot_theme_is_ascii_and_preserves_whiptail_as_the_ui_engine() {
    let wrapper = repo_file("build/usb/includes.chroot/usr/lib/rigos/rigos-firstboot-whiptail");

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

#[cfg(unix)]
#[test]
fn firstboot_theme_wrapper_maps_buttons_and_preserves_exit_status() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_nanos();
    let temporary = std::env::temp_dir().join(format!(
        "rigos-firstboot-theme-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&temporary).expect("failed to create theme test directory");

    let backend = temporary.join("fake-whiptail");
    let capture = temporary.join("arguments.txt");
    fs::write(
        &backend,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >\"$RIGOS_THEME_CAPTURE\"\nexit \"${RIGOS_THEME_EXIT:-0}\"\n",
    )
    .expect("failed to write fake whiptail backend");
    let mut permissions = fs::metadata(&backend)
        .expect("failed to stat fake whiptail backend")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&backend, permissions)
        .expect("failed to make fake whiptail backend executable");

    let wrapper = repo_path("build/usb/includes.chroot/usr/lib/rigos/rigos-firstboot-whiptail");
    let status = Command::new("sh")
        .arg(&wrapper)
        .args([
            "--title",
            "RIGOS FIRST BOOT",
            "--menu",
            "Select Flight Sheet",
            "20",
            "76",
            "2",
            "manual",
            "Configure manually",
            "none",
            "Leave mining unconfigured",
        ])
        .env("RIGOS_WHIPTAIL_REAL", &backend)
        .env("RIGOS_THEME_CAPTURE", &capture)
        .env("RIGOS_THEME_EXIT", "7")
        .env("RIGOS_FIRSTBOOT_BACKTITLE", "TEST BACKTITLE")
        .status()
        .expect("failed to execute firstboot theme wrapper");

    assert_eq!(
        status.code(),
        Some(7),
        "wrapper must preserve backend exit status"
    );

    let arguments =
        fs::read_to_string(&capture).expect("fake whiptail backend did not capture arguments");
    let arguments: Vec<_> = arguments.lines().collect();
    assert_eq!(
        &arguments[..8],
        [
            "--backtitle",
            "TEST BACKTITLE",
            "--ok-button",
            "SELECT",
            "--cancel-button",
            "BACK",
            "--title",
            "RIGOS FIRST BOOT",
        ]
    );
    assert!(
        arguments
            .windows(2)
            .any(|pair| pair == ["--menu", "Select Flight Sheet"])
    );
    assert!(
        arguments
            .windows(2)
            .any(|pair| pair == ["manual", "Configure manually"])
    );
    assert!(
        arguments
            .windows(2)
            .any(|pair| pair == ["none", "Leave mining unconfigured"])
    );

    fs::remove_dir_all(&temporary).expect("failed to clean theme test directory");
}
