#!/usr/bin/env bash
set -euo pipefail

export CARGO_TERM_COLOR=always
export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=x86-64"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p rigos-schema --bin generate-schemas -- --check
cargo build --workspace --release --locked

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

echo "DBYTE RIGOS verification passed"
