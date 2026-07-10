use std::fs;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

fn recovery_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/usb/includes.chroot/usr/local/sbin/rigos-recovery-access")
}

fn alpha8_runtime_check_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check-alpha8-runtime.py")
}

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn recovery_password_is_persisted_restored_and_redacted() {
    let root = std::env::temp_dir().join(format!("rigos-recovery-access-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create recovery test directory");
    let program = r#"
import json
import os
import runpy
import stat
import sys
from pathlib import Path

source = Path(sys.argv[1])
root = Path(sys.argv[2])
namespace = runpy.run_path(str(source), run_name='rigos_recovery_access_test')
g = namespace['main'].__globals__
g['RUNTIME'] = root / 'run'
g['STATE'] = root / 'state'
g['CREDENTIAL_DIRECTORY'] = root / 'state' / 'recovery'
g['CREDENTIAL_FILE'] = g['CREDENTIAL_DIRECTORY'] / 'rigosadmin-password.hash'
g['BOOT_ID'] = root / 'boot-id'
g['STATE_STATUS'] = g['RUNTIME'] / 'state-status.json'
g['BOOT_ID'].write_text('boot-test\n', encoding='ascii')
g['RUNTIME'].mkdir()
g['STATE'].mkdir()
g['STATE_STATUS'].write_text(json.dumps({
    'schema': 'rigos.state-status/v1',
    'boot_id': 'boot-test',
    'outcome': 'ready',
    'mountpoint': str(g['STATE']),
}), encoding='utf-8')
g['persistent_store_ready'] = lambda _status: True
valid_hash = '$y$j9T$syntheticSalt$syntheticHashValue'

assert namespace['valid_password_hash'](valid_hash)
for invalid in ('', '!', '*', '!locked', 'bad:hash', 'bad\nhash', '$6$missing'):
    assert not namespace['valid_password_hash'](invalid)

# Fresh setup prompts once, then persists the hash atomically with strict modes.
live_ready = {'value': False}
prompts = []
persisted = []
g['password_ready'] = lambda: live_ready['value']
def prompt(_invalid, persistent):
    assert persistent is True
    prompts.append(True)
    live_ready['value'] = True
g['prompt_for_password'] = prompt
g['current_password_hash'] = lambda: valid_hash if live_ready['value'] else None
real_persist = g['persist_password_hash']
def persist(value):
    persisted.append(value)
    real_persist(value)
g['persist_password_hash'] = persist
g['unit_active'] = lambda _name: False
g['unit_enabled'] = lambda _name: False
assert namespace['main']() == 0
assert len(prompts) == 1 and persisted == [valid_hash]
assert stat.S_IMODE(g['CREDENTIAL_DIRECTORY'].stat().st_mode) == 0o700
assert stat.S_IMODE(g['CREDENTIAL_FILE'].stat().st_mode) == 0o600
status = json.loads((g['RUNTIME'] / 'recovery-access-status.json').read_text())
assert status['credential_action'] == 'created'
assert status['credential_scope'] == 'persistent'
assert status['credential_persisted'] is True
assert valid_hash not in json.dumps(status)

# Reboot simulation restores through chpasswd without setup UI or passwd.
live_ready['value'] = False
prompts.clear()
calls = []
class Result:
    def __init__(self, returncode=0): self.returncode = returncode
def fake_run(argv, **kwargs):
    calls.append((argv, kwargs))
    if argv == ['/usr/sbin/chpasswd', '--encrypted']:
        assert kwargs['input'] == 'rigosadmin:' + valid_hash + '\n'
        assert valid_hash not in argv
        live_ready['value'] = True
    return Result()
g['subprocess'].run = fake_run
g['persist_password_hash'] = real_persist
assert namespace['main']() == 0
assert not prompts
assert not any('/usr/bin/passwd' in argv for argv, _kwargs in calls)
status = json.loads((g['RUNTIME'] / 'recovery-access-status.json').read_text())
assert status['credential_action'] == 'restored'
assert status['credential_scope'] == 'persistent'
assert status['credential_persisted'] is True
assert valid_hash not in json.dumps(status)

# Existing live credential migrates without a prompt.
g['CREDENTIAL_FILE'].unlink()
live_ready['value'] = True
prompts.clear()
assert namespace['main']() == 0
assert not prompts and g['CREDENTIAL_FILE'].read_text().strip() == valid_hash

# An invalid store is never sent to chpasswd and enters explicit replacement setup.
g['CREDENTIAL_FILE'].write_text('!unsafe\n', encoding='ascii')
g['CREDENTIAL_FILE'].chmod(0o600)
live_ready['value'] = False
invalid_flags = []
def replacement(invalid, persistent):
    invalid_flags.append((invalid, persistent))
    live_ready['value'] = True
g['prompt_for_password'] = replacement
calls.clear()
assert namespace['main']() == 0
assert invalid_flags == [(True, True)]
assert not any(argv == ['/usr/sbin/chpasswd', '--encrypted'] for argv, _kwargs in calls)
assert valid_hash not in json.dumps(json.loads((g['RUNTIME'] / 'recovery-access-status.json').read_text()))

# Unready state uses a truthful boot-scoped credential and never touches the store.
g['CREDENTIAL_FILE'].unlink(missing_ok=True)
g['STATE_STATUS'].write_text(json.dumps({
    'schema': 'rigos.state-status/v1',
    'boot_id': 'boot-test',
    'outcome': 'repair_required',
    'mountpoint': None,
}), encoding='utf-8')
g['persistent_store_ready'] = lambda _status: False
live_ready['value'] = False
boot_prompts = []
def boot_prompt(invalid, persistent):
    assert invalid is False and persistent is False
    boot_prompts.append(True)
    live_ready['value'] = True
g['prompt_for_password'] = boot_prompt
g['persist_password_hash'] = lambda _value: (_ for _ in ()).throw(
    AssertionError('boot credential touched persistent state')
)
assert namespace['main']() == 0
assert boot_prompts == [True]
assert not g['CREDENTIAL_FILE'].exists()
status = json.loads((g['RUNTIME'] / 'recovery-access-status.json').read_text())
assert status['credential_scope'] == 'boot'
assert status['credential_persisted'] is False
assert status['state_outcome'] == 'repair_required'
assert valid_hash not in json.dumps(status)
"#;
    let result = Command::new("python3")
        .arg("-c")
        .arg(program)
        .arg(recovery_path())
        .arg(&root)
        .status()
        .expect("run recovery access fixture");
    let _ = fs::remove_dir_all(&root);
    assert!(result.success(), "recovery access fixture failed");
}

#[test]
fn alpha8_runtime_authority_is_exact_and_fail_closed() {
    let result = Command::new("python3")
        .arg(alpha8_runtime_check_path())
        .status()
        .expect("run Alpha8 runtime authority fixture");
    assert!(result.success(), "Alpha8 runtime authority fixture failed");
}

#[test]
fn alpha8_appliance_wiring_is_explicit() {
    let hook = fs::read_to_string(repo_path("build/usb/hooks/010-rigos.chroot"))
        .expect("read appliance hook");
    assert!(hook.contains("chmod 0755 /usr/local/bin/rigosd /usr/local/bin/rigosctl"));
    assert!(!hook.contains("ln -sfn /usr/lib/rigos/rigosd /usr/local/bin/rigosd"));
    assert!(!hook.contains("ln -sfn /usr/lib/rigos/rigosctl /usr/local/bin/rigosctl"));
    assert!(hook.contains("/usr/lib/rigos/rigos-runtime-publish"));
    assert!(hook.contains("rigos-runtime-render.service"));
    assert!(hook.contains("systemctl disable ssh.socket"));

    for command in ["rigosd", "rigosctl"] {
        let wrapper = fs::read_to_string(repo_path(&format!(
            "build/usb/includes.chroot/usr/local/bin/{command}"
        )))
        .expect("read inspector wrapper");
        assert!(wrapper.contains("--xmrig-config /run/rigos/xmrig-public.json"));
        assert!(wrapper.contains(&format!("exec /usr/lib/rigos/{command}")));
    }

    let runtime_service = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-runtime-render.service",
    ))
    .expect("read runtime authority service");
    assert!(runtime_service.contains("ExecStart=/usr/lib/rigos/rigos-runtime-authority"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let authority =
            repo_path("build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-authority");
        let mode = fs::metadata(authority).unwrap().permissions();
        assert_ne!(mode.mode() & 0o111, 0);
    }

    let miner = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/rigos-miner.service.d/runtime-render.conf",
    ))
    .expect("read miner runtime override");
    assert!(miner.contains("Requires=rigos-runtime-render.service"));
    assert!(miner.contains("ConditionPathExists=/var/lib/rigos/current"));
    assert!(miner.contains("ExecCondition=+/usr/lib/rigos/rigos-runtime-authority"));
    assert!(miner.contains("ExecCondition=/usr/lib/rigos/rigos-runtime-gate"));
    assert!(miner.contains("ExecStart=/usr/lib/rigos/xmrig -c /run/rigos/xmrig.json"));
    assert!(!miner.contains("--config=/run/rigos/xmrig.json"));

    let ssh = fs::read_to_string(repo_path(
        "build/usb/includes.chroot/etc/systemd/system/ssh.service.d/rigos-observe.conf",
    ))
    .expect("read SSH observer override");
    assert!(ssh.contains("Wants=rigos-remote-access-observe.service"));
}
