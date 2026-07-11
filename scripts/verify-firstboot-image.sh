#!/bin/bash
set -euo pipefail

die() {
    printf 'verify-firstboot-image: %s\n' "$*" >&2
    exit 1
}

[[ $# -eq 1 ]] || die 'usage: verify-firstboot-image.sh <image>'
image="$(readlink -f "$1")"
[[ -f "$image" ]] || die "image is missing: $image"
[[ "$(id -u)" -eq 0 ]] || die 'must run as root'

partition_json="$(sfdisk --json "$image")"
start="$(jq -r '.partitiontable.partitions[1].start' <<<"$partition_json")"
size="$(jq -r '.partitiontable.partitions[1].size' <<<"$partition_json")"
[[ "$start" =~ ^[0-9]+$ && "$size" =~ ^[0-9]+$ ]] || die 'ROOT_A geometry is invalid'

loop="$(
    losetup --find --show --read-only \
        --offset $((start * 512)) \
        --sizelimit $((size * 512)) \
        "$image"
)"
temporary="$(mktemp -d)"
cleanup() {
    set +e
    mountpoint -q "$temporary/root-a" && umount "$temporary/root-a"
    losetup -d "$loop" 2>/dev/null
    rm -rf "$temporary"
}
trap cleanup EXIT

mkdir -p "$temporary/root-a" "$temporary/squash"
mount -o ro "$loop" "$temporary/root-a"
squashfs="$temporary/root-a/live/filesystem.squashfs"
[[ -f "$squashfs" ]] || die 'ROOT_A squashfs is missing'

unsquashfs -no-progress -d "$temporary/squash" "$squashfs" \
    usr/bin/python3 usr/bin/python3.11 usr/bin/whiptail \
    usr/local/sbin/rigos-firstboot \
    usr/lib/rigos/rigos-firstboot-whiptail \
    etc/systemd/system/rigos-firstboot.service \
    etc/systemd/system/rigos-firstboot.service.d/2009-console-theme.conf \
    etc/systemd/system/multi-user.target.wants/rigos-firstboot.service \
    >/dev/null

root="$temporary/squash"
service="$root/etc/systemd/system/rigos-firstboot.service"
dropin="$root/etc/systemd/system/rigos-firstboot.service.d/2009-console-theme.conf"
firstboot="$root/usr/local/sbin/rigos-firstboot"
wrapper="$root/usr/lib/rigos/rigos-firstboot-whiptail"

[[ -L "$root/etc/systemd/system/multi-user.target.wants/rigos-firstboot.service" ]] \
    || die 'firstboot service is not enabled in the appliance'
[[ -x "$root/usr/bin/python3" ]] || die 'Python runtime for firstboot is missing'
[[ -x "$root/usr/bin/whiptail" ]] || die 'whiptail runtime is missing'
[[ -x "$firstboot" ]] || die 'firstboot program is missing or not executable'
[[ -x "$wrapper" ]] || die 'firstboot theme wrapper is missing or not executable'
[[ -f "$dropin" ]] || die 'firstboot theme drop-in is missing'

python3 -m py_compile "$firstboot"
sh -n "$wrapper"

for required in \
    'After=rigos-state.service rigos-state-ready.service rigos-profile-apply.service' \
    'Wants=rigos-state-ready.service' \
    'Before=getty@tty1.service' \
    'ExecCondition=/usr/lib/rigos/rigos-config needs-activation' \
    'ExecStart=/usr/local/sbin/rigos-firstboot' \
    'StandardInput=tty-force' \
    'StandardOutput=tty' \
    'StandardError=journal' \
    'TTYPath=/dev/tty1' \
    'TTYReset=yes' \
    'TTYVTDisallocate=yes'
do
    grep -Fqx "$required" "$service" \
        || die "firstboot service contract is missing: $required"
done

if grep -Fqx 'Requires=rigos-state-ready.service' "$service"; then
    die 'state readiness failure still suppresses firstboot diagnostics'
fi
if grep -Fq 'network-online.target' "$service"; then
    die 'firstboot still depends on network-online'
fi
if grep -Fqx 'StandardError=tty' "$service"; then
    die 'firstboot diagnostics still write over tty1'
fi

for required in \
    "stage='state_not_ready'" \
    "raise FirstbootFailure('state_not_ready')" \
    'def manual_proposal()' \
    "('manual', 'Configure manually')" \
    "raise FirstbootCancelled('mining_left_unconfigured')"
do
    grep -Fq "$required" "$firstboot" \
        || die "firstboot recovery/configuration path is missing: $required"
done

for required in \
    'Environment=RIGOS_WHIPTAIL=/usr/lib/rigos/rigos-firstboot-whiptail' \
    'RIGOS SETUP UTILITY   LOCAL NODE CONFIGURATION'
do
    grep -Fq "$required" "$dropin" \
        || die "firstboot theme drop-in contract is missing: $required"
done

grep -Fq 'RIGOS_WHIPTAIL_REAL:-/usr/bin/whiptail' "$wrapper" \
    || die 'firstboot theme wrapper does not use packaged whiptail'
grep -Fq 'exec "$whiptail_real"' "$wrapper" \
    || die 'firstboot theme wrapper does not preserve the dialog engine'

printf 'RIGOS firstboot exact-image verification passed: %s\n' "$image"
