#!/bin/bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

version_env="$(mktemp)"
cleanup() {
    rm -f "$version_env"
}
trap cleanup EXIT

git -c safe.directory="$repo" show HEAD:build/usb/version.env >"$version_env"
if grep -q $'\r' "$version_env"; then
    printf 'build-usb-image-entrypoint: Git version authority contains CR bytes\n' >&2
    exit 1
fi

# shellcheck disable=SC1090
source "$version_env"

python3 ./scripts/check-alpha8-ssh-hotfix.py
python3 -m py_compile \
    ./build/usb/includes.chroot/usr/lib/rigos/rigos-randomx-msr \
    ./build/usb/includes.chroot/usr/lib/rigos/rigos-miner-gate

export CARGO_TARGET_DIR=/work/rigos-performance-preflight-target
cargo test --locked -p rigos-config --test randomx_msr_authority -- --nocapture

./scripts/build-usb-image.sh

image="./dist/usb/${RIGOS_IMAGE_ID}-${RIGOS_IMAGE_VERSION}.img"
bash ./scripts/verify-randomx-performance-image.sh "$image"
