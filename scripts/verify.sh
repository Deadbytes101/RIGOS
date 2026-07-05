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

firstboot=build/usb/includes.chroot/usr/local/sbin/rigos-firstboot
if rg -q -- '--output-fd' "$firstboot"; then
  echo "first boot redirects whiptail result onto the screen stream" >&2
  exit 1
fi
if ! grep -Fq 'stderr=subprocess.PIPE' "$firstboot"; then
  echo "first boot does not capture whiptail values from stderr" >&2
  exit 1
fi
if grep -Fq 'stdout=subprocess.PIPE' "$firstboot"; then
  echo "first boot hides the whiptail screen" >&2
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
