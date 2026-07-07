use std::fs;
use std::path::PathBuf;

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn image_verifier_requires_runtime_authority_and_stability_bytes() {
    let verifier = fs::read_to_string(repo_path("scripts/verify-usb-appliance.sh")).unwrap();
    let packages =
        fs::read_to_string(repo_path("build/usb/package-lists/rigos.list.chroot")).unwrap();

    assert!(
        packages.lines().any(|line| line.trim() == "jq"),
        "appliance package list is missing jq"
    );

    for required in [
        "usr/bin/jq",
        "jq runtime dependency is missing from the appliance",
        "jq_bin=${RIGOS_JQ:-/usr/bin/jq}",
        "usr/lib/rigos/rigos-runtime-publish",
        "usr/lib/rigos/rigos-runtime-authority",
        "usr/lib/rigos/rigos-miner-health",
        "usr/local/bin/rigosd",
        "usr/local/bin/rigosctl",
        "etc/systemd/system/rigos-runtime-render.service",
        "etc/systemd/system/rigos-miner.service.d/runtime-render.conf",
        "etc/systemd/system/rigos-miner.service.d/stability.conf",
        "etc/systemd/system/rigos-miner-health.timer",
        "ExecStart=/usr/lib/rigos/rigos-runtime-authority",
        "ExecCondition=+/usr/lib/rigos/rigos-runtime-authority",
        "ExecStart=/usr/lib/rigos/xmrig -c /run/rigos/xmrig.json",
        "--xmrig-config /run/rigos/xmrig-public.json",
        "flock -x -w 30",
        "construction: \"allowlist\"",
        "StartLimitBurst=5",
    ] {
        assert!(
            verifier.contains(required),
            "image verifier is missing contract: {required}"
        );
    }
}
