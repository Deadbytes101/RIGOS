use std::fs;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

fn repo(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn identity_seed_validation_is_strict() {
    let root = std::env::temp_dir().join(format!("rigos-seed-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create seed fixture");
    let program = r#"
import json, os, runpy, sys
from pathlib import Path
source, root = Path(sys.argv[1]), Path(sys.argv[2])
ns = runpy.run_path(str(source), run_name='seed_test')
load, SeedError = ns['load_identity_seed'], ns['SeedError']
directory = root / 'rigos' / 'identities'
directory.mkdir(parents=True)
path = directory / 'main-xmr.json'
value = 'SYNTHETIC_IDENTITY_ABC123'
valid = {'schema':'rigos.identity-seed/v1','alias':'main-xmr','kind':'mining_identity','value':value}
path.write_text(json.dumps(valid), encoding='utf-8')
identity = load(root, 'main-xmr')
assert identity['alias'] == 'main-xmr' and identity['value'] == value
for change in (
    {'extra': True},
    {'value': 'bad value'},
    {'value': 'A' * 513},
    {'alias': 'other-xmr'},
):
    candidate = dict(valid); candidate.update(change)
    path.write_text(json.dumps(candidate), encoding='utf-8')
    try: load(root, 'main-xmr'); raise AssertionError('unsafe seed accepted')
    except SeedError: pass
path.write_text(json.dumps(valid), encoding='utf-8')
upper = dict(valid); upper['alias'] = 'MAIN-XMR'; upper['value'] = 'OTHER_IDENTITY'
(directory / 'MAIN-XMR.json').write_text(json.dumps(upper), encoding='utf-8')
try: load(root, 'main-xmr'); raise AssertionError('duplicate alias accepted')
except SeedError: pass
(directory / 'MAIN-XMR.json').unlink()
path.write_text('{"schema":"rigos.identity-seed/v1","alias":"main-xmr","alias":"main-xmr","kind":"mining_identity","value":"ABC"}', encoding='utf-8')
try: load(root, 'main-xmr'); raise AssertionError('duplicate member accepted')
except SeedError: pass
path.unlink(); target = root / 'outside.json'; target.write_text(json.dumps(valid), encoding='utf-8'); os.symlink(target, path)
try: load(root, 'main-xmr'); raise AssertionError('symlink accepted')
except SeedError: pass
"#;
    let status = Command::new("python3")
        .arg("-c")
        .arg(program)
        .arg(repo(
            "build/usb/includes.chroot/usr/lib/rigos/rigos-identity-seed",
        ))
        .arg(&root)
        .status()
        .expect("run seed fixture");
    let _ = fs::remove_dir_all(&root);
    assert!(status.success(), "identity seed fixture failed");
}

#[test]
fn firstboot_resolves_seed_without_value_prompt() {
    let program = r#"
import runpy, sys, tempfile
from pathlib import Path
ns = runpy.run_path(sys.argv[1], run_name='firstboot_seed_test')
g = ns['run'].__globals__
with tempfile.TemporaryDirectory() as temporary:
    g['STATE'] = Path(temporary)
    seed = {'schema':'rigos.identity/v1','alias':'main-xmr','kind':'mining_identity','value':'SYNTHETIC_IDENTITY_ABC123','created_locally':True}
    g['load_identity_seed'] = lambda alias: dict(seed) if alias == 'main-xmr' else None
    g['confirm'] = lambda message, stage=None: True
    g['prompt'] = lambda stage, *_args, **_kwargs: (_ for _ in ()).throw(AssertionError('value prompt used'))
    proposal = {'flight_sheet': {'identity_ref':'main-xmr'}, 'provenance':None}
    assert ns['resolve_identity'](proposal) == seed
"#;
    let status = Command::new("python3")
        .arg("-c")
        .arg(program)
        .arg(repo(
            "build/usb/includes.chroot/usr/local/sbin/rigos-firstboot",
        ))
        .status()
        .expect("run firstboot seed fixture");
    assert!(status.success(), "firstboot seed fixture failed");
}
