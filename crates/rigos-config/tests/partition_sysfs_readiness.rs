use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::process::{Command, Output};
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn image_entrypoint_wires_atomic_partition_sysfs_readiness() {
    let entrypoint =
        fs::read_to_string(repo_path("scripts/build-usb-image-entrypoint.sh")).unwrap();
    let wrapper = fs::read_to_string(repo_path("scripts/rigos-sysfs-cat-wrapper.sh")).unwrap();

    assert!(entrypoint.contains("real_cat=\"$(command -v cat)\""));
    assert!(entrypoint.contains("./scripts/rigos-sysfs-cat-wrapper.sh"));
    assert!(entrypoint.contains("export RIGOS_REAL_CAT=\"$real_cat\""));
    assert!(entrypoint.contains("--test partition_sysfs_readiness"));

    assert!(wrapper.contains("RIGOS_SYSFS_RETRY_ATTEMPTS:-100"));
    assert!(wrapper.contains("/sys/class/block/loop*p[1-4]/dev"));
    assert!(wrapper.contains("^[0-9]+:[0-9]+$"));
    assert!(wrapper.contains("RIGOS partition sysfs device ready after %s attempts:"));
    assert!(wrapper.contains("partition sysfs device did not become readable after"));
}

#[cfg(unix)]
struct TemporaryDirectory {
    path: PathBuf,
}

#[cfg(unix)]
impl TemporaryDirectory {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rigos-partition-sysfs-readiness-{}-{timestamp}",
            std::process::id()
        ));

        fs::create_dir(&path).unwrap();
        Self { path }
    }
}

#[cfg(unix)]
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(unix)]
fn write_fake_cat(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/bash
set -euo pipefail

counter="${RIGOS_SYSFS_TEST_COUNTER:?}"
mode="${RIGOS_SYSFS_TEST_MODE:-retry}"
attempt=0

if [[ -f "$counter" ]]; then
    IFS= read -r attempt <"$counter"
fi

attempt="$((attempt + 1))"
printf '%s\n' "$attempt" >"$counter"

case "$mode" in
    retry)
        if ((attempt < 3)); then
            exit 1
        fi
        printf '7:1\n'
        ;;
    invalid_then_ready)
        if ((attempt < 3)); then
            printf 'not-a-device\n'
            exit 0
        fi
        printf '7:2\n'
        ;;
    fail)
        exit 1
        ;;
    passthrough)
        printf '%s\n' "$*"
        ;;
    *)
        exit 64
        ;;
esac
"#,
    )
    .unwrap();

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
fn run_wrapper(
    wrapper: &Path,
    real: &Path,
    counter: &Path,
    mode: &str,
    attempts: &str,
    arguments: &[&str],
) -> Output {
    Command::new("/bin/bash")
        .arg(wrapper)
        .args(arguments)
        .env("RIGOS_REAL_CAT", real)
        .env("RIGOS_SYSFS_RETRY_ATTEMPTS", attempts)
        .env("RIGOS_SYSFS_TEST_COUNTER", counter)
        .env("RIGOS_SYSFS_TEST_MODE", mode)
        .output()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn sysfs_cat_wrapper_retries_read_failures_and_invalid_values_only_for_partition_dev() {
    let temporary = TemporaryDirectory::new();
    let real = temporary.path.join("real-cat");
    let counter = temporary.path.join("attempts.txt");
    let wrapper = repo_path("scripts/rigos-sysfs-cat-wrapper.sh");
    let sysfs_device = "/sys/class/block/loop0p1/dev";

    write_fake_cat(&real);

    let retry = run_wrapper(&wrapper, &real, &counter, "retry", "5", &[sysfs_device]);

    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&retry.stdout), "7:1\n");
    assert_eq!(fs::read_to_string(&counter).unwrap(), "3\n");
    assert!(
        String::from_utf8_lossy(&retry.stderr)
            .contains("RIGOS partition sysfs device ready after 3 attempts:")
    );

    fs::remove_file(&counter).unwrap();

    let invalid_then_ready = run_wrapper(
        &wrapper,
        &real,
        &counter,
        "invalid_then_ready",
        "5",
        &[sysfs_device],
    );

    assert!(invalid_then_ready.status.success());
    assert_eq!(String::from_utf8_lossy(&invalid_then_ready.stdout), "7:2\n");
    assert_eq!(fs::read_to_string(&counter).unwrap(), "3\n");

    fs::remove_file(&counter).unwrap();

    let passthrough = run_wrapper(
        &wrapper,
        &real,
        &counter,
        "passthrough",
        "5",
        &["/etc/os-release"],
    );

    assert!(passthrough.status.success());
    assert_eq!(
        String::from_utf8_lossy(&passthrough.stdout),
        "/etc/os-release\n"
    );
    assert_eq!(fs::read_to_string(&counter).unwrap(), "1\n");

    fs::remove_file(&counter).unwrap();

    let failure = run_wrapper(&wrapper, &real, &counter, "fail", "2", &[sysfs_device]);

    assert!(!failure.status.success());
    assert!(
        String::from_utf8_lossy(&failure.stderr)
            .contains("partition sysfs device did not become readable after 2 attempts:")
    );
    assert_eq!(fs::read_to_string(&counter).unwrap(), "2\n");
}
