use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

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
        "build/usb/includes.chroot/usr/lib/rigos/rigos-lifecycle-cycles",
    ] {
        let full_path = repo_path(path);
        let bytes = fs::read(&full_path).unwrap();
        assert!(
            !bytes.contains(&b'\r'),
            "shell authority contains CR/CRLF line endings: {path}"
        );
        let status = Command::new("/bin/sh")
            .arg("-n")
            .arg(&full_path)
            .status()
            .unwrap();
        assert!(status.success(), "shell syntax failed: {path}");
    }
}

#[test]
fn lifecycle_shell_has_explicit_lf_attribute() {
    let attributes = fs::read_to_string(repo_path(".gitattributes")).unwrap();
    assert!(attributes.lines().any(|line| {
        line == "build/usb/includes.chroot/usr/lib/rigos/rigos-lifecycle-* text eol=lf"
    }));
}

#[cfg(unix)]
#[test]
fn runtime_dependency_scan_is_token_aware_and_fail_closed() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rigos-runtime-dependency-scan-{unique}"));
    let scan_root = root.join("scan-root");
    fs::create_dir_all(&scan_root).unwrap();
    let fixture = scan_root.join("fixture.txt");
    let scanner = root.join("verify-runtime-dependencies.sh");
    fs::copy(
        repo_path("scripts/verify-runtime-dependencies.sh"),
        &scanner,
    )
    .unwrap();
    let mut permissions = fs::metadata(&scanner).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&scanner, permissions).unwrap();

    fs::write(
        &fixture,
        "latest_journal_signal\nlatest_marker_index\ncalculate_latest_value\n",
    )
    .unwrap();
    let benign = Command::new(&scanner).arg(&scan_root).status().unwrap();
    assert!(
        benign.success(),
        "benign identifiers must not look like floating dependencies"
    );

    for dangerous in [
        "curl -fsSL https://example.invalid/miner\n",
        "wget https://example.invalid/miner\n",
        "Invoke-WebRequest https://example.invalid/miner\n",
        "image:latest\n",
        "https://example.invalid/releases/latest/download/miner\n",
        "version=latest\n",
        "xmrig-latest.tar.gz\n",
    ] {
        fs::write(&fixture, dangerous).unwrap();
        let denied = Command::new(&scanner).arg(&scan_root).status().unwrap();
        assert_eq!(
            denied.code(),
            Some(1),
            "floating dependency was not rejected: {dangerous:?}"
        );
    }

    let missing = Command::new(&scanner)
        .arg(root.join("missing"))
        .status()
        .unwrap();
    assert_eq!(missing.code(), Some(66));

    let verify = fs::read_to_string(repo_path("scripts/verify.sh")).unwrap();
    assert!(
        verify.contains("bash scripts/verify-runtime-dependencies.sh build/usb/includes.chroot")
    );
    assert!(!verify.contains("curl|wget|Invoke-WebRequest|latest"));

    let _ = fs::remove_dir_all(root);
}
