use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn runtime_authority_shells_are_lf_and_parse_cleanly() {
    for path in [
        "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-publish",
        "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-authority",
    ] {
        let full_path = repo_path(path);
        let bytes = fs::read(&full_path).unwrap();
        assert!(
            !bytes.contains(&b'\r'),
            "shell authority contains CR/CRLF line endings: {path}"
        );
        let status = Command::new("sh")
            .arg("-n")
            .arg(&full_path)
            .status()
            .unwrap();
        assert!(status.success(), "shell syntax failed: {path}");
    }
}
