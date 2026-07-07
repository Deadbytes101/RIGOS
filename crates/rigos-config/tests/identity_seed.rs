use std::fs;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

fn repository_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn identity_seed_is_bounded_strict_and_redacted() {
    let root = std::env::temp_dir().join(format!("rigos-identity-seed-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create identity seed test directory");
    let program = r#"
import json
import os
import runpy
import stat
import sys
from pathlib import Path

source = Path(sys.argv[1])
root = Path(sys.argv[2])
namespace = runpy.run_path(str(source), run_name='rigos_identity_seed_test')
load = namespace['load_identity_seed']
write_private = namespace['write_private_json']
SeedError = namespace['SeedError']
identity_dir = root / 'rigos' / 'identities'
identity_dir.mkdir(parents=True)
value = 'SYNTHETIC_PUBLIC_WALLET_ABC123'
valid = {
    'schema': 'rigos.identity-seed/v1',
    'alias': 'main-xmr',
    'kind': 'mining_identity',
    'value': value,
}
path = identity_dir / 'main-xmr.json'
path.write_text(json.dumps(valid), encoding='utf-8')
identity = load(root, 'main-xmr')
assert identity == {
    'schema': 'rigos.identity/v1',
    'alias': 'main-xmr',
    'kind': 'mining_identity',
    'value': value,
    'created_locally': True,
}
output = root / 'private.json'
write_private(output, identity)
assert stat.S_IMODE(output.stat().st_mode) == 0o600

# Unknown fields are rejected without reflecting the value.
invalid = dict(valid)
invalid['unexpected'] = True
path.write_text(json.dumps(invalid), encoding='utf-8')
try:
    load(root, 'main-xmr')
    raise AssertionError('unknown field was accepted')
except SeedError as error:
    assert value not in str(error)

# Duplicate aliases are rejected case insensitively.
path.write_text(json.dumps(valid), encoding='utf-8')
upper = dict(valid)
upper['alias'] = 'MAIN-XMR'
upper['value'] = 'SYNTHETIC_PUBLIC_WALLET_DEF456'
(identity_dir / 'MAIN-XMR.json').write_text(json.dumps(upper), encoding='utf-8')
try:
    load(root, 'main-xmr')
    raise AssertionError('duplicate alias was accepted')
except SeedError:
    pass
(identity_dir / 'MAIN-XMR.json').unlink()

# Filename and alias must agree.
wrong = dict(valid)
wrong['alias'] = 'other-xmr'
path.write_text(json.dumps(wrong), encoding='utf-8')
try:
    load(root, 'main-xmr')
    raise AssertionError('filename alias mismatch was accepted')
except SeedError:
    pass

# Oversized and control-bearing values are rejected.
for bad_value in ('A' * 513, 'bad value', 'bad\nvalue', ''):
    bad = dict(valid)
    bad['value'] = bad_value
    path.write_text(json.dumps(bad), encoding='utf-8')
    try:
        load(root, 'main-xmr')
        raise AssertionError('unsafe value was accepted')
    except SeedError:
        pass

# Duplicate JSON members are rejected.
path.write_text(
    '{"schema":"rigos.identity-seed/v1","alias":"main-xmr",'
    '"alias":"main-xmr","kind":"mining_identity","value":"ABC123"}',
    encoding='utf-8',
)
try:
    load(root, 'main-xmr')
    raise AssertionError('duplicate JSON member was accepted')
except SeedError:
    pass

# Symlinks are rejected and never followed.
path.unlink()
target = root / 'outside.json'
target.write_text(json.dumps(valid), encoding='utf-8')
os.symlink(target, path)
try:
    load(root, 'main-xmr')
    raise AssertionError('identity seed symlink was accepted')
except SeedError:
    pass
"#;
    let status = Command::new("python3")
        .arg("-c")
        .arg(program)
        .arg(repository_path(
            "build/usb/includes.chroot/usr/lib/rigos/rigos-identity-seed",
        ))
        .arg(&root)
        .status()
        .expect("run identity seed fixture");
    let _ = fs::remove_dir_all(&root);
    assert!(status.success(), "identity seed fixture failed");
}

#[test]
fn seeded_firstboot_avoids_identity_typing_and_fails_closed_on_conflict() {
    let root = std::env::temp_dir().join(format!("rigos-seeded-firstboot-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create seeded firstboot test directory");
    let program = r#"
import json
import runpy
import sys
from pathlib import Path

firstboot_source = Path(sys.argv[1])
wrapper_source = Path(sys.argv[2])
root = Path(sys.argv[3])
firstboot = runpy.run_path(str(firstboot_source), run_name='rigos_firstboot_seed_test')
wrapper = runpy.run_path(str(wrapper_source), run_name='rigos_firstboot_seed_wrapper_test')
fg = firstboot['run'].__globals__
wg = wrapper['install_seed_resolver'].__globals__
fg['STATE'] = root / 'state'
fg['STATE'].mkdir()
seed_value = 'SYNTHETIC_PUBLIC_WALLET_ABC123'
seed = {
    'schema': 'rigos.identity/v1',
    'alias': 'main-xmr',
    'kind': 'mining_identity',
    'value': seed_value,
    'created_locally': True,
}
wg['invoke_identity_seed'] = lambda alias: dict(seed) if alias == 'main-xmr' else None
fg['prompt'] = lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError('identity prompt used'))
fg['dialog'] = lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError('identity dialog used'))
wrapper['install_seed_resolver'](firstboot)
proposal = {
    'flight_sheet': {'identity_ref': 'main-xmr'},
    'provenance': None,
}
resolved = firstboot['resolve_identity'](proposal)
assert resolved == seed
assert proposal['flight_sheet']['identity_ref'] == 'main-xmr'

# A persistent alias with a different value is not silently replaced.
identity_dir = fg['STATE'] / 'identities'
identity_dir.mkdir()
conflict = dict(seed)
conflict['value'] = 'SYNTHETIC_PUBLIC_WALLET_CONFLICT'
(identity_dir / 'main-xmr.json').write_text(json.dumps(conflict), encoding='utf-8')
fg['dialog'] = lambda *_args, **_kwargs: ''
try:
    firstboot['resolve_identity']({'flight_sheet': {'identity_ref': 'main-xmr'}, 'provenance': None})
    raise AssertionError('conflicting persistent identity was replaced')
except firstboot['FirstbootFailure'] as error:
    assert error.reason == 'identity_seed_conflict'
"#;
    let status = Command::new("python3")
        .arg("-c")
        .arg(program)
        .arg(repository_path(
            "build/usb/includes.chroot/usr/local/sbin/rigos-firstboot",
        ))
        .arg(repository_path(
            "build/usb/includes.chroot/usr/local/sbin/rigos-firstboot-seeded",
        ))
        .arg(&root)
        .status()
        .expect("run seeded firstboot fixture");
    let _ = fs::remove_dir_all(&root);
    assert!(status.success(), "seeded firstboot fixture failed");
}
