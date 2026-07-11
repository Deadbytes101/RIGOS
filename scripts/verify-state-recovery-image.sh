#!/bin/bash
set -euo pipefail

die() {
    printf 'verify-state-recovery-image: %s\n' "$*" >&2
    exit 1
}

[[ $# -eq 1 ]] || die 'usage: verify-state-recovery-image.sh <image>'
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
    usr/bin/python3 usr/bin/python3.11 \
    usr/lib/rigos/rigos-state-init \
    usr/local/sbin/rigos-state-orchestrate \
    usr/local/sbin/rigos-recovery-access \
    usr/lib/rigos/rigos-recovery-access-verify \
    etc/systemd/system/rigos-state.service \
    etc/rigos-release \
    >/dev/null

root="$temporary/squash"
orchestrator="$root/usr/local/sbin/rigos-state-orchestrate"
state_init="$root/usr/lib/rigos/rigos-state-init"
recovery="$root/usr/local/sbin/rigos-recovery-access"
gate="$root/usr/lib/rigos/rigos-recovery-access-verify"
state_service="$root/etc/systemd/system/rigos-state.service"
release="$root/etc/rigos-release"

[[ -x "$root/usr/bin/python3" ]] || die 'Python runtime is missing'
[[ -x "$state_init" ]] || die 'state initializer is missing or not executable'
[[ -x "$orchestrator" ]] || die 'state orchestrator is missing or not executable'
[[ -x "$recovery" ]] || die 'recovery credential authority is missing or not executable'
[[ -f "$gate" ]] || die 'recovery credential gate is missing'
[[ -f "$state_service" ]] || die 'state service is missing'
[[ -f "$release" ]] || die 'release metadata is missing'

python3 -m py_compile "$orchestrator" "$recovery" "$gate"
grep -Fqx 'VERSION_ID="0.0.4-alpha.18"' "$release" \
    || die 'embedded alpha.18 version is missing'
grep -Fqx 'TimeoutStartSec=20min' "$state_service" \
    || die 'state service full repair window is missing'

for required in \
    'FILESYSTEM_TIMEOUT_SECONDS = 300' \
    'E2FSCK_UNCORRECTED_EXIT = 4' \
    'E2FSCK_EXIT_RE = re.compile' \
    'def e2fsck_exit_code(message: str) -> int | None:' \
    'def repair_ext4(device: Path, failure_prefix: str) -> bool:' \
    'def complete_resize_after_timeout() -> bool:' \
    'timeout=FILESYSTEM_TIMEOUT_SECONDS' \
    'automatic ext4 repair failed' \
    'state filesystem resize failed' \
    'resize2fs: timeout' \
    '["/usr/sbin/e2fsck", "-f", "-y"' \
    'e2fsck_exit == E2FSCK_UNCORRECTED_EXIT' \
    'SYS_DEV_BLOCK = Path("/sys/dev/block")' \
    'MAJOR_MINOR_RE = re.compile' \
    'def attested_state_device(' \
    'stat.S_ISBLK' \
    'state_sysfs.parent != disk_sysfs' \
    'PARTUUID symlink resolved away from attested state device'
do
    grep -Fq "$required" "$orchestrator" \
        || die "state repair contract is missing: $required"
done

for required in \
    'Some\((?P<option>[0-9]+)\)' \
    'def revalidate_state_device(expected: Path)' \
    'verified state device changed during repair' \
    'or e2fsck_exit_code(check_failure) != E2FSCK_UNCORRECTED_EXIT' \
    'attested state path major:minor changed' \
    'attested state block is not a child of the attested boot disk' \
    'except FileNotFoundError:'
do
    grep -Fq "$required" "$orchestrator" \
        || die "state repair safety boundary is missing: $required"
done

for required in \
    'def persistent_store_ready(status: dict) -> bool:' \
    '"credential_scope": "persistent" if persistent else "boot"' \
    'credential_persisted = persistent and CREDENTIAL_FILE.is_file()' \
    'This password is not persistent'
do
    grep -Fq "$required" "$recovery" \
        || die "recovery credential contract is missing: $required"
done

for required in \
    'scope = status.get("credential_scope")' \
    'if scope == "persistent":' \
    'elif scope == "boot":' \
    'boot_credential_claims_persistence'
do
    grep -Fq "$required" "$gate" \
        || die "recovery credential gate contract is missing: $required"
done

printf 'RIGOS state recovery and credential image verification passed: %s\n' "$image"
