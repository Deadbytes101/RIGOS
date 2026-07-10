use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn repo_file(path: &str) -> String {
    let path = repo_path(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn diagnostic_ssh_keeps_host_identity_mandatory_without_requiring_ready_state() {
    let service =
        repo_file("build/usb/includes.chroot/etc/systemd/system/rigos-ssh-hostkeys.service");
    let ssh_dropin =
        repo_file("build/usb/includes.chroot/etc/systemd/system/ssh.service.d/rigos-observe.conf");
    let policy =
        repo_file("build/usb/includes.chroot/etc/ssh/sshd_config.d/01-rigos-hostkeys.conf");
    let authority = repo_file("build/usb/includes.chroot/usr/lib/rigos/rigos-ssh-hostkeys");
    let version = repo_file("build/usb/version.env");

    assert!(service.contains("After=rigos-state-ready.service"));
    assert!(service.contains("Wants=rigos-state-ready.service"));
    assert!(!service.contains("Requires=rigos-state-ready.service"));
    assert!(service.contains("Before=ssh.service"));
    assert!(ssh_dropin.contains("Requires=rigos-ssh-hostkeys.service"));

    for required in [
        "HostKey /run/rigos/ssh-hostkeys/ssh_host_ed25519_key",
        "PasswordAuthentication yes",
        "PermitRootLogin no",
        "AllowUsers rigosadmin",
    ] {
        assert!(policy.lines().any(|line| line == required));
    }

    for required in [
        "ACTIVE_KEYS = RUNTIME / \"ssh-hostkeys\"",
        "mode = \"persistent\"",
        "mode = \"ephemeral\"",
        "rigos.ssh-active-hostkeys/v1",
        "persistent_state_ready",
        "install_active_keyset",
    ] {
        assert!(
            authority.contains(required),
            "authority is missing {required}"
        );
    }

    assert!(version.contains("RIGOS_IMAGE_VERSION=0.0.4-alpha.11"));
    assert!(version.contains("RIGOS_BUILD_ORDINAL=11"));
}

#[test]
fn hostkey_authority_selects_ephemeral_or_persistent_storage_without_touching_unready_state() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rigos-diagnostic-ssh-{unique}"));
    fs::create_dir_all(&root).unwrap();

    let fixture = r#"
import runpy
import sys
from pathlib import Path

source = Path(sys.argv[1])
root = Path(sys.argv[2])
namespace = runpy.run_path(str(source), run_name='rigos_ssh_hostkeys_test')
g = namespace['main'].__globals__


def configure(case):
    runtime = root / case / 'run'
    state = root / case / 'state'
    runtime.mkdir(parents=True)
    g['RUNTIME'] = runtime
    g['STATE'] = state
    g['SYSTEM'] = state / 'system'
    g['KEYS'] = state / 'system' / 'ssh-hostkeys'
    g['PRIVATE_KEY'] = g['KEYS'] / 'ssh_host_ed25519_key'
    g['PUBLIC_KEY'] = g['KEYS'] / 'ssh_host_ed25519_key.pub'
    g['MANIFEST'] = g['KEYS'] / 'manifest.json'
    g['ACTIVE_KEYS'] = runtime / 'ssh-hostkeys'
    g['ACTIVE_PRIVATE_KEY'] = g['ACTIVE_KEYS'] / 'ssh_host_ed25519_key'
    g['ACTIVE_PUBLIC_KEY'] = g['ACTIVE_KEYS'] / 'ssh_host_ed25519_key.pub'
    g['ACTIVE_MANIFEST'] = g['ACTIVE_KEYS'] / 'manifest.json'
    g['STATUS'] = runtime / 'ssh-hostkeys-status.json'
    g['STATE_STATUS'] = runtime / 'state-status.json'
    g['current_boot_id'] = lambda: 'boot-test'
    g['ensure_runtime_root'] = lambda: None
    return state


# State unavailable: only a boot-scoped runtime identity may be selected.
state = configure('ephemeral')
recorded = {}
def unavailable(_boot_id):
    raise g['AuthorityError']('limited_capacity')
g['validate_state_ready'] = unavailable
g['install_active_keyset'] = lambda boot_id, mode, *args: (
    recorded.update({'boot_id': boot_id, 'mode': mode, 'args': args}) or 'SHA256:ephemeral'
)
g['write_status'] = lambda boot_id, outcome, **extra: recorded.update(
    {'status_boot_id': boot_id, 'outcome': outcome, **extra}
)
assert namespace['main']() == 0
assert recorded['mode'] == 'ephemeral'
assert recorded['persistence_mode'] == 'ephemeral'
assert recorded['state_reason'] == 'limited_capacity'
assert not state.exists(), 'ephemeral fallback touched unverified persistent state'


# Verified state: preserve the persistent identity and publish it to runtime.
state = configure('persistent')
recorded = {}
g['validate_state_ready'] = lambda _boot_id: None
g['secure_state_root'] = lambda: None
g['ensure_secure_directory'] = lambda _path, _mode: None
g['generate_persistent_keyset'] = lambda _boot_id: 'SHA256:persistent'
def publish(boot_id, mode, private_key, public_key, source_fingerprint):
    recorded.update({
        'boot_id': boot_id,
        'mode': mode,
        'private_key': private_key,
        'public_key': public_key,
        'source_fingerprint': source_fingerprint,
    })
    return source_fingerprint
g['install_active_keyset'] = publish
g['write_status'] = lambda boot_id, outcome, **extra: recorded.update(
    {'status_boot_id': boot_id, 'outcome': outcome, **extra}
)
assert namespace['main']() == 0
assert recorded['mode'] == 'persistent'
assert recorded['persistence_mode'] == 'persistent'
assert recorded['source_fingerprint'] == 'SHA256:persistent'
assert recorded['state_reason'] is None
"#;

    let result = Command::new("python3")
        .arg("-c")
        .arg(fixture)
        .arg(repo_path(
            "build/usb/includes.chroot/usr/lib/rigos/rigos-ssh-hostkeys",
        ))
        .arg(&root)
        .status()
        .expect("run diagnostic SSH host-key fixture");

    let _ = fs::remove_dir_all(root);
    assert!(result.success(), "diagnostic SSH host-key fixture failed");
}
