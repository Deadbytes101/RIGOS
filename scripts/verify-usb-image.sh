#!/bin/bash
set -euo pipefail

die() { printf 'verify-usb-image: %s\n' "$*" >&2; exit 1; }
[[ $# -eq 1 ]] || die 'usage: verify-usb-image.sh <rigos.iso>'
iso="$(readlink -f "$1")"
[[ -f "$iso" ]] || die "image not found: $iso"

signature="$(od -An -tx1 -j510 -N2 "$iso" | tr -d ' \n')"
[[ "$signature" == 55aa ]] || die 'hybrid MBR signature is missing'

boot_report="$(xorriso -indev "$iso" -report_el_torito plain 2>&1)"
grep -Eq 'El Torito boot img .* BIOS .* y ' <<<"$boot_report" || die 'bootable BIOS entry is missing'
grep -Eq 'El Torito boot img .* UEFI .* y ' <<<"$boot_report" || die 'bootable UEFI entry is missing'

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
xorriso -osirrox on -indev "$iso" \
  -extract /live/filesystem.squashfs "$temporary/filesystem.squashfs" >/dev/null 2>&1
unsquashfs -no-progress -d "$temporary/root" "$temporary/filesystem.squashfs" \
  usr/lib/rigos/xmrig \
  usr/local/sbin/rigos-firstboot \
  usr/local/sbin/rigos-state-mount \
  etc/systemd/system/rigos-firstboot.service \
  etc/systemd/system/rigos-miner.service \
  etc/systemd/system/rigos-state.service >/dev/null

[[ -x "$temporary/root/usr/lib/rigos/xmrig" ]] || die 'XMRig is missing or not executable'
[[ -x "$temporary/root/usr/local/sbin/rigos-firstboot" ]] || die 'first-boot TUI is missing'
"$temporary/root/usr/lib/rigos/xmrig" --version | grep -q '^XMRig 6\.26\.0$' \
  || die 'unexpected XMRig version'

printf 'RIGOS USB image verification passed: %s\n' "$iso"
