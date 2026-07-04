#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export RIGOS_BUILD_COMMIT="$(git rev-parse --verify HEAD 2>/dev/null || printf unknown)"
export RUSTFLAGS="-C target-cpu=x86-64"
export SOURCE_DATE_EPOCH="$(git log -1 --format=%ct 2>/dev/null || printf 0)"

cargo build --release --locked --target x86_64-unknown-linux-gnu
mkdir -p dist
cp target/x86_64-unknown-linux-gnu/release/rigosd dist/rigosd
if command -v objcopy >/dev/null 2>&1; then
  objcopy --only-keep-debug dist/rigosd dist/rigosd.debug
  objcopy --strip-debug --add-gnu-debuglink=dist/rigosd.debug dist/rigosd
fi
sha256sum dist/rigosd > dist/rigosd.sha256

