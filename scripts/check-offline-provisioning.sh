#!/bin/bash
set -euo pipefail

root="${1:-build/usb/includes.chroot}"
unit="$root/etc/systemd/system/rigos-firstboot.service"
seed="$root/usr/lib/rigos/rigos-identity-seed"
firstboot="$root/usr/local/sbin/rigos-firstboot"

[[ -f "$unit" ]] || { echo 'firstboot unit missing' >&2; exit 1; }
[[ -f "$seed" ]] || { echo 'identity seed resolver missing' >&2; exit 1; }
[[ -f "$firstboot" ]] || { echo 'firstboot missing' >&2; exit 1; }
grep -Fq 'ExecStart=/usr/local/sbin/rigos-firstboot' "$unit"
grep -Fq 'rigos.identity-seed/v1' "$seed"
grep -Fq 'ro,nodev,nosuid,noexec' "$seed"
grep -Fq 'O_NOFOLLOW' "$seed"
grep -Fq 'load_identity_seed' "$firstboot"
grep -Fq 'identity_seed_confirmation' "$firstboot"
python3 -m py_compile "$seed" "$firstboot"
printf 'RIGOS offline provisioning source check passed\n'
