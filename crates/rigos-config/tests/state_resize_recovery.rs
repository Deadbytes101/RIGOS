use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn state_resize_timeout_has_a_bounded_verified_recovery_path() {
    let orchestrator = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/usr/local/sbin/rigos-state-orchestrate",
    ))
    .unwrap();
    let service = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-state.service",
    ))
    .unwrap();

    for required in [
        "FILESYSTEM_TIMEOUT_SECONDS = 300",
        "def complete_resize_after_timeout() -> bool:",
        "timeout=FILESYSTEM_TIMEOUT_SECONDS",
        "post-timeout ext4 check failed",
        "state filesystem resize failed",
        "resize2fs: timeout",
    ] {
        assert!(
            orchestrator.contains(required),
            "state recovery contract is missing: {required}"
        );
    }
    assert!(service.contains("TimeoutStartSec=12min"));
}

#[test]
fn resize_timeout_recovery_retries_core_and_reports_failed_repair_truthfully() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rigos-state-resize-{unique}"));
    fs::create_dir_all(&root).unwrap();

    let fixture = r#"
import runpy
import sys
from pathlib import Path

source = Path(sys.argv[1])
root = Path(sys.argv[2])
namespace = runpy.run_path(str(source), run_name='rigos_state_resize_test')
g = namespace['main'].__globals__
g['STATUS'] = root / 'state-status.json'
g['ATTESTATION'] = root / 'boot-device.json'
g['BOOT_ID'] = root / 'boot-id'
g['BOOT_ID'].write_text('boot-test\n', encoding='ascii')

# A core resize timeout is completed under the longer verified repair budget,
# then core is rerun to perform the normal mount and initialization path.
statuses = [
    {'outcome': 'limited_capacity', 'message': 'bounded command failed: resize2fs: timeout'},
    {'outcome': 'ready', 'message': None},
]
core_calls = []
repair_calls = []
g['run_core'] = lambda: core_calls.append(True) or 0
def read_json(path):
    if path == g['STATUS']:
        return statuses.pop(0)
    return {}
g['read_json'] = read_json
g['complete_resize_after_timeout'] = lambda: repair_calls.append(True) or True
assert namespace['main']() == 0
assert len(core_calls) == 2
assert repair_calls == [True]
assert statuses == []

# A failed long repair is not reclassified as limited capacity.
recorded = []
g['verified_state_device'] = lambda: (Path('/dev/test-state'), None)
results = iter([(True, None), (False, 'resize2fs: timeout after 300s')])
g['run_repair_command'] = lambda _argv, _accepted: next(results)
g['mark_repair_required'] = lambda message: recorded.append(message)
assert namespace['complete_resize_after_timeout']() is False
assert recorded == ['state filesystem resize failed: resize2fs: timeout after 300s']
"#;

    let result = Command::new("python3")
        .arg("-c")
        .arg(fixture)
        .arg(repo_path(
            "build/usb/includes.chroot/usr/local/sbin/rigos-state-orchestrate",
        ))
        .arg(&root)
        .status()
        .expect("run state resize recovery fixture");

    let _ = fs::remove_dir_all(root);
    assert!(result.success(), "state resize recovery fixture failed");
}
