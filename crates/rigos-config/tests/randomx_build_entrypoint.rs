use std::fs;
use std::path::PathBuf;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn performance_entrypoint_uses_exact_lf_git_version_authority() {
    let attributes = fs::read_to_string(repo_path(".gitattributes")).unwrap();
    let entrypoint =
        fs::read_to_string(repo_path("scripts/build-usb-image-entrypoint.sh")).unwrap();

    assert!(
        attributes
            .lines()
            .any(|line| line == "build/usb/version.env text eol=lf")
    );
    assert!(entrypoint.contains(
        "git -c safe.directory=\"$repo\" show HEAD:build/usb/version.env >\"$version_env\""
    ));
    assert!(entrypoint.contains("if grep -q $'\\r' \"$version_env\"; then"));
    assert!(entrypoint.contains("source \"$version_env\""));
    assert!(!entrypoint.contains("source ./build/usb/version.env"));
    assert!(entrypoint.contains("rigos-randomx-msr"));
    assert!(entrypoint.contains("rigos-miner-gate"));
}
