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
    let entrypoint =
        fs::read_to_string(repo_path("scripts/build-usb-image-entrypoint.sh")).unwrap();
    let image_verifier =
        fs::read_to_string(repo_path("scripts/verify-state-recovery-image.sh")).unwrap();
    let state_init = fs::read_to_string(repo_path("crates/rigos-state/src/main.rs")).unwrap();

    for required in [
        "FILESYSTEM_TIMEOUT_SECONDS = 300",
        "E2FSCK_UNCORRECTED_EXIT = 4",
        "E2FSCK_EXIT_RE = re.compile",
        "def e2fsck_exit_code(message: str) -> int | None:",
        "def repair_ext4(device: Path, failure_prefix: str) -> bool:",
        "def complete_resize_after_timeout() -> bool:",
        "timeout=FILESYSTEM_TIMEOUT_SECONDS",
        "automatic ext4 repair failed",
        "state filesystem resize failed",
        "resize2fs: timeout",
        "[\"/usr/sbin/e2fsck\", \"-f\", \"-y\"",
        "e2fsck_exit == E2FSCK_UNCORRECTED_EXIT",
        "SYS_DEV_BLOCK = Path(\"/sys/dev/block\")",
        "MAJOR_MINOR_RE = re.compile",
        "def attested_state_device(",
        "stat.S_ISBLK",
        "state_sysfs.parent != disk_sysfs",
        "PARTUUID symlink resolved away from attested state device",
    ] {
        assert!(
            orchestrator.contains(required),
            "state recovery contract is missing: {required}"
        );
        assert!(
            image_verifier.contains(required),
            "exact-image verifier is missing: {required}"
        );
    }
    assert!(service.contains("TimeoutStartSec=20min"));
    assert!(image_verifier.contains("losetup --find --show --read-only"));
    assert!(image_verifier.contains("mount -o ro"));
    assert!(!image_verifier.contains("mount -o rw"));
    assert!(entrypoint.contains("bash ./scripts/verify-state-recovery-image.sh \"$image\""));

    for required in [
        "const FILESYSTEM_TIMEOUT: Duration = Duration::from_secs(300);",
        "const SEED_STATE_UUID: &str = \"dc450e72-daa4-5b82-8d1b-0ae6b11607f9\";",
        "fn prepare_state_filesystem(device: &str) -> Result<(), InitError>",
        "run_filesystem(\"e2fsck\", &[\"-p\", device], None, &[0, 1])",
        "run_filesystem(\"resize2fs\", &[device], None, &[0])",
        "run_filesystem(",
        "\"tune2fs\"",
        "&[\"-U\", \"random\", \"-L\", \"RIGOS_STATE\", device]",
        "fn inspect_state_filesystem(device: &str) -> Result<FilesystemIdentity, InitError>",
        "state filesystem label is not RIGOS_STATE after identity update",
        "state filesystem UUID still matches the cloned seed UUID",
        "InitError::RepairRequired(format!(\"{program}: timeout after 300s\"))",
        "StateOutcome::RepairRequired",
    ] {
        assert!(
            state_init.contains(required),
            "state initializer transaction contract is missing: {required}"
        );
    }
}

#[test]
fn resize_timeout_recovery_retries_core_and_repairs_only_e2fsck_exit_four() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rigos-state-resize-{unique}"));
    fs::create_dir_all(&root).unwrap();

    let fixture = r#"
import json
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

# The parser accepts both the Python helper and Rust Option exit formats only.
assert g['e2fsck_exit_code']('e2fsck: exit 4: inconsistency') == 4
assert g['e2fsck_exit_code']('e2fsck: exit Some(4): inconsistency') == 4
assert g['e2fsck_exit_code']('e2fsck: exit Some(8): operational error') == 8
assert g['e2fsck_exit_code']('e2fsck: exit None: signal') is None

# The attested-path fallback requires exact path, major:minor, and disk parent identity.
attestation = {
    'schema': 'rigos.boot-device/v1',
    'boot_id': 'boot-test',
    'verification_outcome': 'verified',
    'disk': {'path': '/dev/test-disk', 'major_minor': '8:16'},
    'state': {
        'path': '/dev/test-state',
        'major_minor': '8:20',
        'partuuid': '5249474f-04',
    },
}
state = attestation['state']
real_resolve_strict = g['resolve_strict']
g['resolve_strict'] = lambda _path: Path('/dev/test-state')
g['block_device_major_minor'] = lambda _path: ('8:20', None)
g['state_belongs_to_attested_disk'] = lambda state_mm, disk_mm: (
    state_mm == '8:20' and disk_mm == '8:16',
    None,
)
device, error = g['attested_state_device'](attestation, state)
assert device == Path('/dev/test-state')
assert error is None
g['block_device_major_minor'] = lambda _path: ('8:21', None)
device, error = g['attested_state_device'](attestation, state)
assert device is None
assert error == 'attested state path major:minor changed'
g['resolve_strict'] = real_resolve_strict

# Missing PARTUUID link may use only the already-verified attested block path.
links = root / 'by-partuuid'
links.mkdir()
g['PARTUUID_ROOT'] = links
g['attested_state_device'] = lambda _attestation, _state: (Path('/dev/test-state'), None)
g['ATTESTATION'].write_text(json.dumps(attestation), encoding='utf-8')
class Result:
    def __init__(self, returncode, stdout=''):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = ''
def fake_run(argv, **_kwargs):
    if argv[0] == '/usr/bin/findmnt':
        return Result(1)
    if argv[0] == '/usr/sbin/blkid':
        return Result(0, 'TYPE=ext4\nLABEL=RIGOS_STATE_SEED\nPARTUUID=5249474f-04\n')
    raise AssertionError(f'unexpected command: {argv}')
g['subprocess'].run = fake_run
device, error = g['verified_state_device']()
assert device == Path('/dev/test-state')
assert error is None

# An existing PARTUUID link that points elsewhere is rejected, never used as fallback.
other = root / 'other-state'
other.touch()
(links / '5249474f-04').symlink_to(other)
device, error = g['verified_state_device']()
assert device is None
assert error == 'PARTUUID symlink resolved away from attested state device'

# A core resize timeout is completed under the longer verified repair budget,
# then core is rerun to perform the normal mount and initialization path.
statuses = [
    {'outcome': 'limited_capacity', 'message': 'bounded command failed: resize2fs: timeout'},
    {'outcome': 'ready', 'message': None},
]
core_calls = []
resize_calls = []
g['run_core'] = lambda: core_calls.append(True) or 0
def read_json(path):
    if path == g['STATUS']:
        return statuses.pop(0)
    return {}
g['read_json'] = read_json
g['complete_resize_after_timeout'] = lambda: resize_calls.append(True) or True
assert namespace['main']() == 0
assert len(core_calls) == 2
assert resize_calls == [True]
assert statuses == []

# The exact Rust exit Some(4) message emitted physically reaches verified fsck.
statuses = [
    {
        'outcome': 'limited_capacity',
        'message': 'bounded command failed: e2fsck: exit Some(4): unexpected inconsistency',
    },
    {'outcome': 'ready', 'message': None},
]
core_calls.clear()
forced_calls = []
g['forced_check'] = lambda: forced_calls.append(True) or True
assert namespace['main']() == 0
assert len(core_calls) == 2
assert forced_calls == [True]
assert statuses == []

# Rust exit Some(8) remains operational failure and never enters repair.
statuses = [
    {
        'outcome': 'limited_capacity',
        'message': 'bounded command failed: e2fsck: exit Some(8): operational error',
    },
]
core_calls.clear()
forced_calls.clear()
assert namespace['main']() == 0
assert len(core_calls) == 1
assert forced_calls == []
assert statuses == []

# Preen exit 4 is the only condition that escalates to bounded -y repair.
recorded = []
calls = []
g['verified_state_device'] = lambda: (Path('/dev/test-state'), None)
results = iter([
    (False, 'e2fsck: exit 4: unexpected inconsistency'),
    (True, None),
    (True, None),
])
def run_repair(argv, accepted):
    calls.append((argv, accepted))
    return next(results)
g['run_repair_command'] = run_repair
g['mark_repair_required'] = lambda message: recorded.append(message)
assert namespace['complete_resize_after_timeout']() is True
assert [call[0][0:3] for call in calls] == [
    ['/usr/sbin/e2fsck', '-f', '-p'],
    ['/usr/sbin/e2fsck', '-f', '-y'],
    ['/usr/sbin/resize2fs', '/dev/test-state'],
]
assert recorded == []

# Operational e2fsck failures never escalate to -y.
recorded.clear()
calls.clear()
results = iter([(False, 'e2fsck: exit 8: operational error')])
assert namespace['complete_resize_after_timeout']() is False
assert len(calls) == 1
assert calls[0][0][0:3] == ['/usr/sbin/e2fsck', '-f', '-p']
assert recorded == ['post-timeout ext4 check failed: e2fsck: exit 8: operational error']

# A failed resize remains repair_required and is never reclassified as capacity.
recorded.clear()
calls.clear()
results = iter([(True, None), (False, 'resize2fs: timeout after 300s')])
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
