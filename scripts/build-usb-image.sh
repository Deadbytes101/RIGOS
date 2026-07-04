#!/bin/bash
set -euo pipefail

die() { printf 'build-usb-image: %s\n' "$*" >&2; exit 1; }
[[ "$(id -u)" -eq 0 ]] || die 'must run as root inside the image builder'
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
[[ -z "$(git -c safe.directory="$root" status --porcelain)" ]] || die 'source tree must be clean'

work=/work/rigos-live
rm -rf "$work"
mkdir -p "$work/config/package-lists" "$work/config/includes.chroot" "$work/config/hooks/live"
cp build/usb/package-lists/* "$work/config/package-lists/"
cp -a build/usb/includes.chroot/. "$work/config/includes.chroot/"
cp build/usb/hooks/010-rigos.chroot "$work/config/hooks/live/010-rigos.hook.chroot"
chmod 0755 "$work/config/hooks/live/"* \
  "$work/config/includes.chroot/usr/local/sbin/rigos-firstboot" \
  "$work/config/includes.chroot/usr/local/sbin/rigos-state-mount"

cd "$work"
lb config noauto \
  --mode debian \
  --distribution bookworm \
  --architectures amd64 \
  --binary-images iso-hybrid \
  --bootloaders grub-efi,grub-pc \
  --archive-areas 'main contrib non-free-firmware' \
  --apt-recommends false \
  --debian-installer none \
  --iso-application RIGOS \
  --iso-publisher RIGOS \
  --iso-volume RIGOS_LIVE \
  --bootappend-live 'boot=live components noautologin quiet'
lb build

output="$root/dist/usb"
mkdir -p "$output"
install -m 0644 live-image-amd64.hybrid.iso "$output/rigos-usb-amd64.iso"
(cd "$output" && sha256sum rigos-usb-amd64.iso > rigos-usb-amd64.iso.sha256)
printf 'USB image: %s\n' "$output/rigos-usb-amd64.iso"
