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
    let image_verifier =
        fs::read_to_string(repo_path("scripts/verify-randomx-performance-image.sh")).unwrap();
    let image_hook = fs::read_to_string(repo_path("build/usb/hooks/010-rigos.chroot")).unwrap();

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
    assert!(entrypoint.contains("--test randomx_build_entrypoint"));

    assert!(image_hook.contains("/usr/lib/rigos/rigos-randomx-msr"));
    assert!(image_hook.contains("rigos-randomx-msr.service rigos-miner.service"));

    assert!(image_verifier.contains("msr_support=\"module\""));
    assert!(image_verifier.contains("msr_support=\"builtin\""));
    assert!(image_verifier.contains("modules.builtin"));
    assert!(image_verifier.contains("kernel/arch/x86/kernel/msr\\.ko"));
    assert!(image_verifier.contains("Do not use grep -q in a pipe while pipefail is enabled"));
    assert!(!image_verifier.contains("grep -Eq"));
    assert!(
        image_verifier
            .contains("kernel MSR support is absent from module files and modules.builtin")
    );
}
