#!/bin/bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

python3 ./scripts/check-alpha8-ssh-hotfix.py
python3 -m py_compile ./build/usb/includes.chroot/usr/lib/rigos/rigos-randomx-msr

export CARGO_TARGET_DIR=/work/rigos-performance-preflight-target
cargo test --locked -p rigos-config --test randomx_msr_authority -- --nocapture

./scripts/build-usb-image.sh

# shellcheck disable=SC1091
source ./build/usb/version.env
image="./dist/usb/${RIGOS_IMAGE_ID}-${RIGOS_IMAGE_VERSION}.img"
bash ./scripts/verify-randomx-performance-image.sh "$image"
