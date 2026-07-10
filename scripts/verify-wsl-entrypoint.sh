#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: verify-wsl-entrypoint.sh REPOSITORY" >&2
  exit 64
fi

repo="$1"
if [[ ! -f "$repo/Cargo.toml" || ! -f "$repo/scripts/verify.sh" ]]; then
  echo "RIGOS_WSL_REPOSITORY_INVALID=$repo" >&2
  exit 66
fi

cd "$repo"

if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi

missing=0
for tool in cargo rustc python3 bash sh git grep rg mktemp cmp diff jq; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'RIGOS_WSL_TOOL_MISSING=%s\n' "$tool" >&2
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  echo "Install the missing tool inside this WSL distribution, then rerun scripts/verify-wsl.ps1." >&2
  exit 127
fi

for component in fmt clippy; do
  if ! cargo "$component" --version >/dev/null 2>&1; then
    printf 'RIGOS_WSL_CARGO_COMPONENT_MISSING=%s\n' "$component" >&2
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  echo "Install the missing Rust component inside this WSL distribution, then rerun scripts/verify-wsl.ps1." >&2
  exit 127
fi

pycache_root=$(mktemp -d)
cleanup() {
  rm -rf "$pycache_root"
}
trap cleanup EXIT HUP INT TERM
export PYTHONPYCACHEPREFIX="$pycache_root"
export PYTHONDONTWRITEBYTECODE=1

printf 'RIGOS_WSL_REPO=%s\n' "$repo"
printf 'RIGOS_WSL_CARGO=%s\n' "$(command -v cargo)"
bash ./scripts/verify.sh
bash ./scripts/verify-firstboot-theme-wrapper.sh
