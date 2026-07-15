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
fn image_entrypoint_wires_bounded_partition_node_readiness() {
    let entrypoint =
        fs::read_to_string(repo_path("scripts/build-usb-image-entrypoint.sh")).unwrap();
    let wrapper = fs::read_to_string(repo_path("scripts/rigos-blockdev-wrapper.sh")).unwrap();

    assert!(entrypoint.contains("real_blockdev=\"$(command -v blockdev)\""));
    assert!(entrypoint.contains("./scripts/rigos-blockdev-wrapper.sh"));
    assert!(entrypoint.contains("export RIGOS_REAL_BLOCKDEV=\"$real_blockdev\""));
    assert!(entrypoint.contains("export PATH=\"$grub_wrapper_dir:$PATH\""));
    assert!(entrypoint.contains("--test partition_node_readiness"));

    assert!(wrapper.contains("RIGOS_BLOCKDEV_RETRY_ATTEMPTS:-100"));
    assert!(wrapper.contains("--getsize64"));
    assert!(wrapper.contains("/work/rigos-appliance/partition-nodes.*/*p[1-4]"));
    assert!(wrapper.contains("RIGOS partition node ready after %s attempts:"));
    assert!(wrapper.contains("partition block device did not become readable after"));
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
            "rigos-partition-node-readiness-{}-{timestamp}",
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
fn write_fake_blockdev(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/bash
set -euo pipefail

counter="${RIGOS_BLOCKDEV_TEST_COUNTER:?}"
mode="${RIGOS_BLOCKDEV_TEST_MODE:-retry}"
attempt=0

if [[ -f "$counter" ]]; then
    attempt="$(cat "$counter")"
fi

attempt="$((attempt + 1))"
printf '%s\n' "$attempt" >"$counter"

case "$mode" in
    retry)
        if ((attempt < 3)); then
            exit 1
        fi
        printf '524288\n'
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
        .env("RIGOS_REAL_BLOCKDEV", real)
        .env("RIGOS_BLOCKDEV_RETRY_ATTEMPTS", attempts)
        .env("RIGOS_BLOCKDEV_TEST_COUNTER", counter)
        .env("RIGOS_BLOCKDEV_TEST_MODE", mode)
        .output()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn blockdev_wrapper_retries_only_private_partition_size_reads() {
    let temporary = TemporaryDirectory::new();
    let real = temporary.path.join("real-blockdev");
    let counter = temporary.path.join("attempts.txt");
    let wrapper = repo_path("scripts/rigos-blockdev-wrapper.sh");
    let private_node = "/work/rigos-appliance/partition-nodes.test/loop0p1";

    write_fake_blockdev(&real);

    let retry = run_wrapper(
        &wrapper,
        &real,
        &counter,
        "retry",
        "5",
        &["--getsize64", private_node],
    );

    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&retry.stdout), "524288\n");
    assert_eq!(fs::read_to_string(&counter).unwrap(), "3\n");
    assert!(
        String::from_utf8_lossy(&retry.stderr)
            .contains("RIGOS partition node ready after 3 attempts:")
    );

    fs::remove_file(&counter).unwrap();

    let passthrough = run_wrapper(
        &wrapper,
        &real,
        &counter,
        "passthrough",
        "5",
        &["--getss", "/dev/loop0"],
    );

    assert!(passthrough.status.success());
    assert_eq!(
        String::from_utf8_lossy(&passthrough.stdout),
        "--getss /dev/loop0\n"
    );
    assert_eq!(fs::read_to_string(&counter).unwrap(), "1\n");

    fs::remove_file(&counter).unwrap();

    let failure = run_wrapper(
        &wrapper,
        &real,
        &counter,
        "fail",
        "2",
        &["--getsize64", private_node],
    );

    assert!(!failure.status.success());
    assert!(
        String::from_utf8_lossy(&failure.stderr)
            .contains("partition block device did not become readable after 2 attempts:")
    );
    assert_eq!(fs::read_to_string(&counter).unwrap(), "2\n");
}
