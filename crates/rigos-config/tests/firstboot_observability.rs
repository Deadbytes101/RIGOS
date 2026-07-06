use std::fs;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

fn firstboot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot/usr/local/sbin/rigos-firstboot")
}

#[test]
fn firstboot_classifies_dialog_events_without_persisting_values() {
    let root = std::env::temp_dir().join(format!("rigos-firstboot-ui-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create firstboot test directory");
    let program = r#"
import json
import os
import runpy
import signal
import sys
from pathlib import Path

source = Path(sys.argv[1])
root = Path(sys.argv[2])
status_path = root / 'firstboot-ui-status.json'
boot_id_path = root / 'boot-id'
boot_id_path.write_text('boot-test\n', encoding='ascii')
os.environ['RIGOS_FIRSTBOOT_UI_STATUS'] = str(status_path)
os.environ['RIGOS_FIRSTBOOT_BOOT_ID'] = str(boot_id_path)
namespace = runpy.run_path(str(source), run_name='rigos_firstboot_observability_test')
globals_ = namespace['run'].__globals__
globals_['journal'] = lambda _value, _priority: None

class Result:
    def __init__(self, returncode, stderr=''):
        self.returncode = returncode
        self.stderr = stderr


def status():
    return json.loads(status_path.read_text(encoding='utf-8'))


def run_dialog(returncode, stderr=''):
    globals_['subprocess'].run = lambda *_args, **_kwargs: Result(returncode, stderr)
    globals_['main'] = lambda: globals_['dialog'](
        '--menu', 'synthetic', '10', '60', '1', 'item', 'value', stage='synthetic_dialog'
    )
    return globals_['run']()

assert run_dialog(1) == 10
value = status()
assert value['outcome'] == 'cancelled'
assert value['reason'] == 'dialog_cancelled'
assert value['return_code'] == 1

assert run_dialog(255) == 10
value = status()
assert value['outcome'] == 'cancelled'
assert value['reason'] == 'dialog_escaped'

assert run_dialog(255, 'SENTINEL_PRIVATE_DIALOG_VALUE') == 20
value = status()
assert value['outcome'] == 'failed'
assert value['reason'] == 'whiptail_runtime_error'
assert 'SENTINEL_PRIVATE_DIALOG_VALUE' not in json.dumps(value)

assert run_dialog(-signal.SIGHUP) == 20
value = status()
assert value['outcome'] == 'failed'
assert value['reason'] == 'dialog_signal'
assert value['signal'] == 'SIGHUP'

globals_['subprocess'].run = lambda *_args, **_kwargs: Result(1)
assert globals_['confirm']('synthetic confirmation', stage='synthetic_confirmation') is False
value = status()
assert value['outcome'] == 'declined'
assert value['reason'] == 'user_selected_no'

globals_['main'] = lambda: None
assert globals_['run']() == 0
value = status()
assert value['outcome'] == 'ready'
assert value['reason'] == 'firstboot_completed'
assert value['stage'] == 'complete'
assert set(value) == {
    'schema', 'boot_id', 'outcome', 'stage', 'dialog', 'reason',
    'return_code', 'signal'
}
"#;
    let result = Command::new("python3")
        .arg("-c")
        .arg(program)
        .arg(firstboot_path())
        .arg(&root)
        .status()
        .expect("run firstboot observability fixture");
    let _ = fs::remove_dir_all(&root);
    assert!(result.success(), "firstboot observability fixture failed");
}
