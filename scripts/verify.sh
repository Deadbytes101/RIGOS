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

echo "DBYTE RIGOS verification passed"
