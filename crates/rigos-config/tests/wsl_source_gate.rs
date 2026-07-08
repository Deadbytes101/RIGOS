use std::fs;
use std::path::PathBuf;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn wsl_launcher_is_path_safe_and_fail_closed() {
    let launcher = fs::read_to_string(repo_path("scripts/verify-wsl.ps1"))
        .expect("read WSL source gate launcher");

    for required in [
        "$PSScriptRoot",
        "wslpath -a",
        "RIGOS_WSL_DISTRO",
        "command -v \"$tool\"",
        "RIGOS_WSL_TOOL_MISSING",
        "exec bash ./scripts/verify.sh",
        "RIGOS_WSL_SOURCE_GATE=PASS",
    ] {
        assert!(
            launcher.contains(required),
            "WSL launcher contract missing: {required}"
        );
    }

    for forbidden in [
        "/mnt/d/TECHNICAL/dbyte-rigos",
        "curl | sh",
        "Invoke-WebRequest",
        "rustup-init",
    ] {
        assert!(
            !launcher.contains(forbidden),
            "WSL launcher contains forbidden bootstrap or hard-coded path: {forbidden}"
        );
    }
}
