#!/usr/bin/env bash
set -euo pipefail

export CARGO_TERM_COLOR=always
export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=x86-64"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p rigos-schema --bin generate-schemas -- --check
cargo build --workspace --release --locked

bash -n scripts/*.sh build/usb/hooks/*.chroot
python3 -m py_compile build/usb/includes.chroot/usr/local/sbin/rigos-firstboot

grep -Fq 'RIGOS_CONFIG_DUPLICATE_KEY' crates/rigos-config/src/lib.rs
grep -Fq 'RIGOS_CONFIG_BOOT_DEVICE_UNPROVEN' crates/rigos-config/src/main.rs
grep -Fq 'ro,nodev,nosuid,noexec' crates/rigos-config/src/main.rs
grep -Fq 'rigos-state-init' crates/rigos-config/src/main.rs
grep -Fq 'boot_id' crates/rigos-state/src/main.rs
grep -Fq 'major_minor' crates/rigos-state/src/main.rs
grep -Fq 'ptuuid' crates/rigos-state/src/main.rs
grep -Fq 'partuuid' crates/rigos-state/src/main.rs
grep -Fq '.pending-transaction.json' crates/rigos-config/src/main.rs
grep -Fq 'engine("transact"' build/usb/includes.chroot/usr/local/sbin/rigos-firstboot
grep -Fq 'ExecCondition=/usr/lib/rigos/rigos-config gate' build/usb/includes.chroot/etc/systemd/system/rigos-miner.service
if rg -n '(HIVE_HOST_URL|API_HOST_URLS|RIG_PASSWD|HSSH_SRV)=' configs docs/local-rig-config.md; then
  echo "Hive cloud or rig credential field leaked into RIGOS configuration" >&2
  exit 1
fi

firstboot=build/usb/includes.chroot/usr/local/sbin/rigos-firstboot
if rg -q -- '--output-fd' "$firstboot"; then
  echo "first boot redirects whiptail result onto the screen stream" >&2
  exit 1
fi
if ! grep -Fq 'stderr=subprocess.PIPE' "$firstboot"; then
  echo "first boot does not capture whiptail values from stderr" >&2
  exit 1
fi
if ! grep -Fq 'return result.stderr.strip()' "$firstboot"; then
  echo "first boot does not read the selected value from stderr" >&2
  exit 1
fi
python3 - "$firstboot" <<'PY'
import runpy
import sys

namespace = runpy.run_path(sys.argv[1], run_name="rigos_firstboot_verify")
subprocess_module = namespace["subprocess"]
real_run = subprocess_module.run
seen = {}

class Result:
    returncode = 0
    stderr = "selected\n"


def fake_run(argv, **kwargs):
    seen["argv"] = argv
    seen["kwargs"] = kwargs
    return Result()


subprocess_module.run = fake_run
try:
    value = namespace["dialog"]("--inputbox", "test", "1", "1")
finally:
    subprocess_module.run = real_run

if value != "selected":
    raise SystemExit("first boot dialog did not return the selected value")
if "--output-fd" in seen["argv"]:
    raise SystemExit("first boot dialog rewired the whiptail output stream")
if seen["kwargs"].get("stderr") is not subprocess_module.PIPE:
    raise SystemExit("first boot dialog does not capture stderr")
if "stdout" in seen["kwargs"]:
    raise SystemExit("first boot dialog does not leave stdout on tty")
PY

python3 - "$firstboot" <<'PY'
import json
import runpy
import sys
import tempfile
from pathlib import Path

source = Path(sys.argv[1]).read_text(encoding="utf-8")
main_source = source[source.index("def main() -> None:"):]
if main_source.index("ensure_administrator_password()") > main_source.index('if state_outcome not in {"ready", "grown"}'):
    raise SystemExit("administrator password is established after the state gate")

namespace = runpy.run_path(sys.argv[1], run_name="rigos_firstboot_identity_verify")
with tempfile.TemporaryDirectory() as temporary:
    state = Path(temporary)
    identities = state / "identities"
    identities.mkdir()
    identity = {"schema":"rigos.identity/v1","alias":"mapped-local","kind":"mining_identity","value":"fixture-private-value","created_locally":True}
    (identities / "mapped-local.json").write_text(json.dumps(identity), encoding="utf-8")
    mapping = {"schema":"rigos.external-identity-map/v1","mappings":[{"source":"hive-style","external_type":"wal_id","external_value":"fixture-external-ref","identity_ref":"mapped-local","confirmed_source_sha256":"fixture-hash"}]}
    (state / "external-identity-map.json").write_text(json.dumps(mapping), encoding="utf-8")
    function_globals = namespace["resolve_identity"].__globals__
    function_globals["STATE"] = state
    function_globals["confirm"] = lambda _message: True
    proposal = {"source_sha256":"fixture-hash","provenance":{"external_reference":{"source":"hive-style","external_type":"wal_id","external_value":"fixture-external-ref"}},"flight_sheet":{"identity_ref":"unresolved"}}
    selected = namespace["resolve_identity"](proposal)
    if selected["alias"] != "mapped-local" or proposal["flight_sheet"]["identity_ref"] != "mapped-local":
        raise SystemExit("confirmed external identity mapping was not reused")

    function_globals["dialog"] = lambda *_args: "existing:mapped-local"
    proposal = {"source_sha256":"native","provenance":None,"flight_sheet":{"identity_ref":"different-native-alias"}}
    selected = namespace["resolve_identity"](proposal)
    if selected["alias"] != "mapped-local" or proposal["flight_sheet"]["identity_ref"] != "mapped-local":
        raise SystemExit("selected identity alias did not update the proposal")
PY

if ! rg -q 'label: dos' scripts/build-usb-image.sh; then
  echo "MBR appliance table declaration missing" >&2
  exit 1
fi
if ! rg -q 'console=ttyS0,115200n8 console=tty0' scripts/build-usb-image.sh; then
  echo "local console is not the final console" >&2
  exit 1
fi
if rg -q 'noapic|noacpi' scripts/build-usb-image.sh; then
  echo "destructive safe mode firmware switches detected" >&2
  exit 1
fi
if ! rg -q 'EFI/BOOT/BOOTX64.EFI' scripts/verify-usb-appliance.sh; then
  echo "removable UEFI verification missing" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cargo run --quiet --locked -p rigosd -- machine inspect --json >"$tmp/machine.json"
cargo run --quiet --locked -p rigosd -- miner inspect --json >"$tmp/miner.json"
cargo run --quiet --locked -p rigosd -- doctor --json >"$tmp/doctor.json"
cargo run --quiet --locked -p rigos-schema --bin validate-cli-output -- "$tmp"

if rg -n 'Command::new\(("|r#")?(sh|bash|curl|wget|ps|pgrep|killall|pkill)' crates; then
  echo "forbidden external command path detected" >&2
  exit 1
fi
if git ls-files | rg '(^|/)(raw|private|work)/|\.(raw\.(json|log)|tar\.zst\.age|age\.partial|pem|key)$'; then
  echo "raw/private validation artifact tracked by Git" >&2
  exit 1
fi
if git grep -n -I -E 'AGE-SECRET-KEY-1[0-9A-Z]{20,}|SENTINEL_SECRET_VALUE' -- ':!scripts/verify.sh'; then
  echo "forbidden secret material detected" >&2
  exit 1
fi
if rg -n -i 'DBYTE RIGOS|dbyte-rigos|dbyte\.rigos|/etc/dbyte-rigos|/var/lib/dbyte-rigos' . \
  -g '!target/**' -g '!docs/historical-rc1-obsolete.md' -g '!scripts/verify.sh'; then
  echo "obsolete pre-release namespace detected" >&2
  exit 1
fi
if rg -n -i '(billing|entitlement|trial_expir|license.{0,12}server|account_balance|payment.{0,12}(gate|control)|remote.{0,12}kill)' \
  crates/rigos-core crates/rigos-machine crates/rigos-miner crates/rigos-xmrig crates/rigosd \
  crates/rigos-pool crates/rigos-evidence crates/rigos-schema; then
  echo "forbidden billing/account/control runtime surface detected" >&2
  exit 1
fi
if rg -n 'if\s+.*(moneroocean|2miners|nicehash|supportxmr|herominers|hashvault|nanopool)' crates/rigos-pool crates/rigosd; then
  echo "pool-name conditional leaked into runtime core" >&2
  exit 1
fi
if rg -n 'curl|wget|Invoke-WebRequest|latest' build/usb/includes.chroot; then
  echo "runtime miner download or floating dependency detected" >&2
  exit 1
fi
for required in build/usb/THIRD_PARTY_NOTICES docs/miner-provenance.md docs/third-party-components.md; do
  [[ -s "$required" ]] || { echo "missing third-party disclosure: $required" >&2; exit 1; }
done
if rg -n -i 'donation.{0,20}disabled|complete mining stack.{0,20}(zero|0%).{0,10}fee|rigos.{0,20}(wallet|donation endpoint)' \
  crates build/usb docs README.md; then
  echo "false fee claim or RIGOS donation destination detected" >&2
  exit 1
fi

echo "RIGOS verification passed"
