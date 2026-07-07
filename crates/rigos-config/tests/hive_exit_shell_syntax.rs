use std::path::PathBuf;
use std::process::Command;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn runtime_authority_shells_parse_cleanly() {
    for path in [
        "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-publish",
        "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-authority",
    ] {
        let status = Command::new("sh")
            .arg("-n")
            .arg(repo_path(path))
            .status()
            .unwrap();
        assert!(status.success(), "shell syntax failed: {path}");
    }
}
