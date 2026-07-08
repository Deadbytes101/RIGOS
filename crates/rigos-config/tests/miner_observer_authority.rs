#[cfg(unix)]
use std::env;
use std::fs;
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn repo_path(path: &str) -> PathBuf {
    repo_root().join(path)
}

#[cfg(unix)]
fn run_python(script: &str) {
    let python = env::var("RIGOS_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let status = Command::new(&python)
        .arg(repo_path(script))
        .current_dir(repo_root())
        .status()
        .unwrap_or_else(|error| panic!("failed to execute {script} with {python}: {error}"));
    assert!(status.success(), "observer source regression failed: {script}");
}

#[cfg(unix)]
#[test]
fn authenticated_miner_observer_behavioral_source_gate() {
    for script in [
        "scripts/test-miner-health-api.py",
        "scripts/test-miner-health-api-authority-errors.py",
        "scripts/test-miner-health-api-schema.py",
        "scripts/test-miner-health-connection-state.py",
        "scripts/test-miner-health-journal-fallback.py",
        "scripts/test-runtime-token-publication.py",
    ] {
        run_python(script);
    }
}

#[test]
fn observer_authority_is_wired_into_build_and_exact_image_gates() {
    let entrypoint = fs::read_to_string(repo_path("scripts/build-usb-image-entrypoint.sh"))
        .expect("read performance image entrypoint");
    let image_verifier = fs::read_to_string(repo_path("scripts/verify-miner-observer-image.sh"))
        .expect("read observer image verifier");
    let observer = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/usr/lib/rigos/rigos-miner-health",
    ))
    .expect("read miner observer");
    let renderer = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-render",
    ))
    .expect("read runtime renderer");
    let publisher = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-publish",
    ))
    .expect("read runtime publisher");

    for script in [
        "test-miner-health-api.py",
        "test-miner-health-api-authority-errors.py",
        "test-miner-health-api-schema.py",
        "test-miner-health-connection-state.py",
        "test-miner-health-journal-fallback.py",
        "test-runtime-token-publication.py",
    ] {
        assert!(
            entrypoint.contains(script),
            "performance image entrypoint does not run {script}"
        );
    }
    assert!(entrypoint.contains("--test miner_observer_authority"));

    for contract in [
        "RIGOS_XMRIG_API_TOKEN_PATH",
        "hashrate_10s",
        "nonnegative_integer",
        "current_hashrate_unavailable",
        "no_current_hashrate_from_api",
        "latest_journal_signal",
        "api_error not in (None, \"api_unavailable\")",
    ] {
        assert!(observer.contains(contract), "observer contract missing: {contract}");
    }

    assert!(renderer.contains("secrets.token_urlsafe(48)"));
    assert!(renderer.contains("127.0.0.1"));
    assert!(renderer.contains("restricted\": True"));
    assert!(publisher.contains("RIGOS_XMRIG_API_TOKEN_PATH=\"$runtime/xmrig-api-token\""));

    for contract in [
        "extracted observer misclassifies disconnected historical hashrate",
        "extracted observer trusts historical hashrate as current",
        "extracted observer trusts stale journal ready evidence",
        "extracted observer hides API authority failure",
        "extracted observer truncates fractional counters",
    ] {
        assert!(
            image_verifier.contains(contract),
            "exact-image behavioral contract missing: {contract}"
        );
    }
}

#[test]
fn unix_only_integrations_are_explicitly_platform_gated() {
    for path in [
        "crates/rigos-config/tests/miner_stability.rs",
        "crates/rigos-config/tests/hive_exit_runtime_publish.rs",
    ] {
        let source = fs::read_to_string(repo_path(path))
            .unwrap_or_else(|error| panic!("read Unix integration {path}: {error}"));
        assert!(
            source.starts_with("#![cfg(unix)]\n"),
            "Unix integration is not explicitly platform gated: {path}"
        );
    }

    let recovery = fs::read_to_string(repo_path("crates/rigos-config/tests/recovery_access.rs"))
        .expect("read recovery access integration");
    let unix_scope = recovery
        .find("#[cfg(unix)]\n    {")
        .expect("recovery executable-mode assertion is not Unix scoped");
    let permissions_import = recovery
        .find("use std::os::unix::fs::PermissionsExt;")
        .expect("recovery Unix permissions import is missing");
    let mode_binding = recovery
        .find("let mode = fs::metadata(authority).unwrap().permissions();")
        .expect("recovery Unix mode binding is missing");
    assert!(unix_scope < permissions_import && permissions_import < mode_binding);
}

#[test]
fn observer_test_files_are_regular_repository_files() {
    for path in [
        "scripts/test-miner-health-api.py",
        "scripts/test-miner-health-api-authority-errors.py",
        "scripts/test-miner-health-api-schema.py",
        "scripts/test-miner-health-connection-state.py",
        "scripts/test-miner-health-journal-fallback.py",
        "scripts/test-runtime-token-publication.py",
    ] {
        let metadata = fs::metadata(repo_path(path)).unwrap_or_else(|error| {
            panic!("observer test file is unavailable: {path}: {error}")
        });
        assert!(metadata.is_file(), "observer test path is not a file: {path}");
    }

    assert!(repo_path("scripts/verify-miner-observer-image.sh").is_file());
}
