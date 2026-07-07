#!/bin/bash
set -euo pipefail

die() {
    printf 'verify-randomx-performance-image: %s\n' "$*" >&2
    exit 1
}

[[ $# -eq 1 ]] || die 'usage: verify-randomx-performance-image.sh <image>'
image="$(readlink -f "$1")"
[[ -f "$image" ]] || die "image is missing: $image"
[[ "$(id -u)" -eq 0 ]] || die 'must run as root'

partition_json="$(sfdisk --json "$image")"
start="$(jq -r '.partitiontable.partitions[1].start' <<<"$partition_json")"
size="$(jq -r '.partitiontable.partitions[1].size' <<<"$partition_json")"
[[ "$start" =~ ^[0-9]+$ && "$size" =~ ^[0-9]+$ ]] || die 'ROOT_A geometry is invalid'

loop="$(losetup --find --show --read-only --offset $((start * 512)) --sizelimit $((size * 512)) "$image")"
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
    usr/bin/python3 usr/bin/python3.11 usr/sbin/modprobe \
    usr/lib/rigos/rigos-randomx-msr \
    etc/systemd/system/rigos-randomx-msr.service \
    etc/systemd/system/rigos-miner.service \
    etc/systemd/system/rigos-miner.service.d/randomx-msr.conf \
    >/dev/null

root="$temporary/squash"
[[ -x "$root/usr/bin/python3" ]] || die 'Python runtime for MSR authority is missing'
[[ -x "$root/usr/sbin/modprobe" ]] || die 'modprobe runtime for MSR authority is missing'
[[ -f "$root/usr/lib/rigos/rigos-randomx-msr" ]] || die 'RandomX MSR authority is missing'
python3 -m py_compile "$root/usr/lib/rigos/rigos-randomx-msr"

service="$root/etc/systemd/system/rigos-randomx-msr.service"
dropin="$root/etc/systemd/system/rigos-miner.service.d/randomx-msr.conf"
miner="$root/etc/systemd/system/rigos-miner.service"
authority="$root/usr/lib/rigos/rigos-randomx-msr"

for required in \
    'ExecStartPre=-/usr/sbin/modprobe msr' \
    'ExecStart=/usr/bin/python3 /usr/lib/rigos/rigos-randomx-msr apply' \
    'ExecStop=/usr/bin/python3 /usr/lib/rigos/rigos-randomx-msr restore' \
    'CapabilityBoundingSet=CAP_SYS_MODULE CAP_SYS_RAWIO' \
    'ReadWritePaths=/run/rigos -/dev/cpu'
do
    grep -Fqx "$required" "$service" || die "MSR service contract is missing: $required"
done

grep -Fqx 'Wants=rigos-randomx-msr.service' "$dropin" || die 'miner does not want optional MSR authority'
grep -Fqx 'After=rigos-randomx-msr.service' "$dropin" || die 'miner is not ordered after MSR authority'
if grep -Fq 'Requires=rigos-randomx-msr.service' "$dropin"; then
    die 'optional MSR optimization incorrectly blocks the miner'
fi
grep -Fqx 'User=rigos' "$miner" || die 'miner no longer runs as the unprivileged rigos user'

for required in \
    'SUPPORTED_CPUS = {("GenuineIntel", 6, 42)}' \
    'REGISTER = 0x1A4' \
    'TARGET_VALUE = 0xF' \
    'apply_failed_rolled_back' \
    'apply_failed_rollback_incomplete' \
    'stale_state_discarded'
do
    grep -Fq "$required" "$authority" || die "MSR authority contract is missing: $required"
done

printf 'RIGOS RandomX performance image verification passed: %s\n' "$image"
