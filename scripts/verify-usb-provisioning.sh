#!/bin/bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
die(){ printf 'verify-usb-provisioning: %s\n' "$*" >&2; exit 1; }
[[ $# -eq 2 ]] || die 'usage: verify-usb-provisioning.sh <image> <manifest>'
image="$(readlink -f "$1")"
manifest="$(readlink -f "$2")"

"$script_dir/verify-usb-appliance.sh" "$image" "$manifest"

partition_json="$(sfdisk --json "$image")"
start="$(jq -r '.partitiontable.partitions[1].start' <<<"$partition_json")"
size="$(jq -r '.partitiontable.partitions[1].size' <<<"$partition_json")"
[[ "$start" =~ ^[0-9]+$ && "$size" =~ ^[0-9]+$ ]] || die 'ROOT_A partition geometry is invalid'

loop="$(losetup --find --show --read-only --offset $((start * 512)) --sizelimit $((size * 512)) "$image")"
temporary="$(mktemp -d)"
cleanup(){
  set +e
  mountpoint -q "$temporary/root-a" && umount "$temporary/root-a"
  losetup -d "$loop" 2>/dev/null
  rm -rf "$temporary"
}
trap cleanup EXIT
mkdir -p "$temporary/root-a" "$temporary/appliance"
mount -o ro,nodev,nosuid,noexec "$loop" "$temporary/root-a"
[[ -f "$temporary/root-a/live/filesystem.squashfs" ]] || die 'ROOT_A squashfs is missing'

unsquashfs -no-progress -d "$temporary/appliance" \
  "$temporary/root-a/live/filesystem.squashfs" \
  etc/systemd/system/rigos-firstboot.service \
  usr/lib/rigos/rigos-identity-seed \
  usr/local/sbin/rigos-firstboot \
  usr/local/sbin/rigos-firstboot-seeded >/dev/null

unit="$temporary/appliance/etc/systemd/system/rigos-firstboot.service"
seed="$temporary/appliance/usr/lib/rigos/rigos-identity-seed"
wrapper="$temporary/appliance/usr/local/sbin/rigos-firstboot-seeded"
original="$temporary/appliance/usr/local/sbin/rigos-firstboot"

[[ -x "$seed" ]] || die 'identity seed verifier is missing or not executable'
[[ -x "$wrapper" ]] || die 'seeded firstboot launcher is missing or not executable'
[[ -x "$original" ]] || die 'original firstboot engine is missing or not executable'
grep -Fq 'ExecStart=/usr/local/sbin/rigos-firstboot-seeded' "$unit" || die 'seeded firstboot launcher is not wired'
grep -Fq 'rigos.identity-seed/v1' "$seed" || die 'identity seed schema is missing'
grep -Fq 'ro,nodev,nosuid,noexec' "$seed" || die 'identity seed mount restrictions are missing'
grep -Fq 'O_NOFOLLOW' "$seed" || die 'identity seed symlink protection is missing'
grep -Fq 'identity_seed_conflict' "$wrapper" || die 'persistent identity conflict gate is missing'
grep -Fq 'invoke_identity_seed' "$wrapper" || die 'identity seed resolver is not installed'
python3 -m py_compile "$seed" "$wrapper" "$original"

printf 'RIGOS offline provisioning verification passed: %s\n' "$image"
