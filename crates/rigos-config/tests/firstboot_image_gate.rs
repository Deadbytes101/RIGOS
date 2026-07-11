use std::fs;
use std::path::PathBuf;

fn repo_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn alpha16_build_runs_source_and_exact_image_firstboot_gates() {
    let version = repo_file("build/usb/version.env");
    let entrypoint = repo_file("scripts/build-usb-image-entrypoint.sh");
    let verifier = repo_file("scripts/verify-firstboot-image.sh");
    let hook = repo_file("build/usb/hooks/010-rigos.chroot");

    assert!(version.contains("RIGOS_PRODUCT_VERSION=0.0.4-alpha.16"));
    assert!(version.contains("RIGOS_IMAGE_VERSION=0.0.4-alpha.16"));
    assert!(version.contains("RIGOS_BUILD_ORDINAL=16"));

    assert!(entrypoint.contains("--test firstboot_tty"));
    assert!(entrypoint.contains("--test state_resize_recovery"));
    assert!(entrypoint.contains("bash ./scripts/verify-firstboot-image.sh \"$image\""));

    for required in [
        "multi-user.target.wants/rigos-firstboot.service",
        "firstboot service is not enabled in the appliance",
        "state readiness failure still suppresses firstboot diagnostics",
        "firstboot still depends on network-online",
        "StandardInput=tty-force",
        "def manual_proposal()",
    ] {
        assert!(
            verifier.contains(required),
            "firstboot exact-image verifier is missing: {required}"
        );
    }

    assert!(
        hook.contains("rigos-firstboot.service"),
        "image construction must enable firstboot"
    );
}

#[test]
fn firstboot_image_gate_is_read_only_against_the_appliance_image() {
    let verifier = repo_file("scripts/verify-firstboot-image.sh");

    assert!(verifier.contains("losetup --find --show --read-only"));
    assert!(verifier.contains("mount -o ro"));
    assert!(!verifier.contains("mount -o rw"));
    assert!(!verifier.contains("systemctl start"));
    assert!(!verifier.contains("systemctl enable"));
}
