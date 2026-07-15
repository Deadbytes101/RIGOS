#!/bin/bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

version_env="$(mktemp)"
grub_wrapper_dir="$(mktemp -d)"

cleanup() {
    rm -f "$version_env"
    rm -rf "$grub_wrapper_dir"
}
trap cleanup EXIT

git -c safe.directory="$repo" show HEAD:build/usb/version.env >"$version_env"
if grep -q $'\r' "$version_env"; then
    printf 'build-usb-image-entrypoint: Git version authority contains CR bytes\n' >&2
    exit 1
fi

# shellcheck disable=SC1090
source "$version_env"

real_grub_install="$(command -v grub-install)"
real_blockdev="$(command -v blockdev)"
real_cat="$(command -v cat)"

[[ "$real_grub_install" == /* && -x "$real_grub_install" ]] || {
    printf 'build-usb-image-entrypoint: real grub-install is unavailable\n' >&2
    exit 1
}

[[ "$real_blockdev" == /* && -x "$real_blockdev" ]] || {
    printf 'build-usb-image-entrypoint: real blockdev is unavailable\n' >&2
    exit 1
}

[[ "$real_cat" == /* && -x "$real_cat" ]] || {
    printf 'build-usb-image-entrypoint: real cat is unavailable\n' >&2
    exit 1
}

install -m 0755 \
    ./scripts/rigos-grub-install-wrapper.sh \
    "$grub_wrapper_dir/grub-install"

install -m 0755 \
    ./scripts/rigos-blockdev-wrapper.sh \
    "$grub_wrapper_dir/blockdev"

install -m 0755 \
    ./scripts/rigos-sysfs-cat-wrapper.sh \
    "$grub_wrapper_dir/cat"

export RIGOS_REAL_GRUB_INSTALL="$real_grub_install"
export RIGOS_REAL_BLOCKDEV="$real_blockdev"
export RIGOS_REAL_CAT="$real_cat"
export PATH="$grub_wrapper_dir:$PATH"

python3 ./scripts/check-alpha8-ssh-hotfix.py
python3 ./scripts/verify-systemd-ordering.py
python3 -m py_compile \
    ./build/usb/includes.chroot/usr/local/sbin/rigos-recovery-access \
    ./build/usb/includes.chroot/usr/local/sbin/rigos-utility \
    ./build/usb/includes.chroot/usr/lib/rigos/rigos-admin-password \
    ./build/usb/includes.chroot/usr/local/sbin/rigos-state-orchestrate \
    ./build/usb/includes.chroot/usr/lib/rigos/rigos-recovery-access-verify \
    ./build/usb/includes.chroot/usr/lib/rigos/rigos-randomx-msr \
    ./build/usb/includes.chroot/usr/lib/rigos/rigos-miner-gate \
    ./build/usb/includes.chroot/usr/lib/rigos/rigos-ssh-hostkeys \
    ./build/usb/includes.chroot/usr/lib/rigos/rigos-runtime-render \
    ./build/usb/includes.chroot/usr/lib/rigos/rigos-miner-health \
    ./scripts/test-miner-health-api.py \
    ./scripts/test-miner-health-api-authority-errors.py \
    ./scripts/test-miner-health-api-schema.py \
    ./scripts/test-miner-health-connection-state.py \
    ./scripts/test-miner-health-journal-fallback.py \
    ./scripts/test-runtime-token-publication.py
python3 ./scripts/test-miner-health-api.py
python3 ./scripts/test-miner-health-api-authority-errors.py
python3 ./scripts/test-miner-health-api-schema.py
python3 ./scripts/test-miner-health-connection-state.py
python3 ./scripts/test-miner-health-journal-fallback.py
python3 ./scripts/test-runtime-token-publication.py

export CARGO_TARGET_DIR=/work/rigos-performance-preflight-target
cargo test --locked -p rigos-config --test miner_observer_authority -- --nocapture
cargo test --locked -p rigos-config --test randomx_build_entrypoint -- --nocapture
cargo test --locked -p rigos-config --test bios_grub_bootstrap -- --nocapture
cargo test --locked -p rigos-config --test partition_node_readiness -- --nocapture
cargo test --locked -p rigos-config --test partition_sysfs_readiness -- --nocapture
cargo test --locked -p rigos-config --test randomx_msr_authority -- --nocapture
cargo test --locked -p rigos-config --test firstboot_tty -- --nocapture
cargo test --locked -p rigos-config --test diagnostic_ssh -- --nocapture
cargo test --locked -p rigos-config --test state_resize_recovery -- --nocapture

./scripts/build-usb-image.sh

image="./dist/usb/${RIGOS_IMAGE_ID}-${RIGOS_IMAGE_VERSION}.img"
bash ./scripts/verify-randomx-performance-image.sh "$image"
bash ./scripts/verify-miner-observer-image.sh "$image"
bash ./scripts/verify-firstboot-image.sh "$image"
bash ./scripts/verify-state-recovery-image.sh "$image"
