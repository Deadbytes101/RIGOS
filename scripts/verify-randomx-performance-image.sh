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

listing="$temporary/squashfs.list"
unsquashfs -ll "$squashfs" >"$listing"

if awk '{print $NF}' "$listing" \
    | grep -E '^squashfs-root/etc/ssh/ssh_host_.*_key(\.pub)?$' >/dev/null
then
    die 'appliance image contains a baked SSH host key'
fi

msr_support="missing"
# Do not use grep -q in a pipe while pipefail is enabled. An early grep exit can
# SIGPIPE the producer and turn a real match into a false pipeline failure.
if awk '{print $NF}' "$listing" \
    | grep -E '^squashfs-root/(usr/)?lib/modules/[^/]+/kernel/arch/x86/kernel/msr\.ko(\.(xz|zst|gz))?$' \
        >/dev/null
then
    msr_support="module"
else
    while IFS= read -r builtin_path; do
        if unsquashfs -cat "$squashfs" "$builtin_path" \
            | grep -E '(^|/)kernel/arch/x86/kernel/msr\.ko$' \
                >/dev/null
        then
            msr_support="builtin"
            break
        fi
    done < <(
        awk '{print $NF}' "$listing" \
            | sed -nE 's#^squashfs-root/((usr/)?lib/modules/[^/]+/modules\.builtin)$#\1#p'
    )
fi

[[ "$msr_support" != "missing" ]] \
    || die 'kernel MSR support is absent from module files and modules.builtin'

unsquashfs -no-progress -d "$temporary/squash" "$squashfs" \
    usr/bin/python3 usr/bin/python3.11 usr/bin/kmod usr/sbin/modprobe \
    usr/lib/rigos/rigos-randomx-msr usr/lib/rigos/rigos-miner-gate \
    usr/lib/rigos/rigos-ssh-hostkeys \
    etc/ssh/sshd_config.d/01-rigos-hostkeys.conf \
    etc/systemd/system/rigos-randomx-msr.service \
    etc/systemd/system/rigos-ssh-hostkeys.service \
    etc/systemd/system/ssh.service.d/rigos-observe.conf \
    etc/systemd/system/rigos-miner.service \
    etc/systemd/system/rigos-miner.service.d/randomx-msr.conf \
    etc/systemd/system/multi-user.target.wants/rigos-randomx-msr.service \
    etc/systemd/system/multi-user.target.wants/rigos-ssh-hostkeys.service \
    >/dev/null

root="$temporary/squash"
[[ -x "$root/usr/bin/python3" ]] || die 'Python runtime for authority services is missing'
[[ -x "$root/usr/bin/kmod" ]] || die 'kmod runtime for MSR authority is missing'
[[ -L "$root/usr/sbin/modprobe" || -x "$root/usr/sbin/modprobe" ]] || die 'modprobe entrypoint is missing'
[[ -f "$root/usr/lib/rigos/rigos-randomx-msr" ]] || die 'RandomX MSR authority is missing'
[[ -f "$root/usr/lib/rigos/rigos-miner-gate" ]] || die 'miner safety gate is missing'
[[ -f "$root/usr/lib/rigos/rigos-ssh-hostkeys" ]] || die 'SSH host-key authority is missing'
[[ -L "$root/etc/systemd/system/multi-user.target.wants/rigos-randomx-msr.service" ]] \
    || die 'RandomX MSR authority is not enabled in the appliance'
[[ -L "$root/etc/systemd/system/multi-user.target.wants/rigos-ssh-hostkeys.service" ]] \
    || die 'SSH host-key authority is not enabled in the appliance'
python3 -m py_compile \
    "$root/usr/lib/rigos/rigos-randomx-msr" \
    "$root/usr/lib/rigos/rigos-miner-gate" \
    "$root/usr/lib/rigos/rigos-ssh-hostkeys"

service="$root/etc/systemd/system/rigos-randomx-msr.service"
dropin="$root/etc/systemd/system/rigos-miner.service.d/randomx-msr.conf"
miner="$root/etc/systemd/system/rigos-miner.service"
authority="$root/usr/lib/rigos/rigos-randomx-msr"
miner_gate="$root/usr/lib/rigos/rigos-miner-gate"
hostkey_service="$root/etc/systemd/system/rigos-ssh-hostkeys.service"
hostkey_authority="$root/usr/lib/rigos/rigos-ssh-hostkeys"
hostkey_policy="$root/etc/ssh/sshd_config.d/01-rigos-hostkeys.conf"
ssh_dropin="$root/etc/systemd/system/ssh.service.d/rigos-observe.conf"

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
    die 'optional MSR optimization incorrectly blocks the baseline miner path'
fi
grep -Fqx 'User=rigos' "$miner" || die 'miner no longer runs as the unprivileged rigos user'
grep -Fqx 'ExecCondition=/usr/lib/rigos/rigos-miner-gate' "$miner" \
    || die 'miner safety gate is not wired'

for required in \
    'After=rigos-state-ready.service' \
    'Wants=rigos-state-ready.service' \
    'Before=ssh.service' \
    'ExecStart=/usr/lib/rigos/rigos-ssh-hostkeys' \
    'ReadWritePaths=/var/lib/rigos /run/rigos'
do
    grep -Fqx "$required" "$hostkey_service" \
        || die "SSH host-key service contract is missing: $required"
done
if grep -Fqx 'Requires=rigos-state-ready.service' "$hostkey_service"; then
    die 'SSH diagnostics are hard-blocked by persistent state readiness'
fi
for required in \
    'HostKey /run/rigos/ssh-hostkeys/ssh_host_ed25519_key' \
    'PasswordAuthentication yes' \
    'PermitRootLogin no' \
    'AllowUsers rigosadmin'
do
    grep -Fqx "$required" "$hostkey_policy" \
        || die "sshd diagnostic access policy is missing: $required"
done
grep -Fqx 'Requires=rigos-ssh-hostkeys.service' "$ssh_dropin" \
    || die 'ssh.service does not require an established host identity'
grep -Fqx 'After=rigos-recovery-access.service rigos-ssh-hostkeys.service' "$ssh_dropin" \
    || die 'ssh.service ordering bypasses host identity establishment'
for required in \
    'STATE = Path("/var/lib/rigos")' \
    'KEYS = SYSTEM / "ssh-hostkeys"' \
    'ACTIVE_KEYS = RUNTIME / "ssh-hostkeys"' \
    '"schema": "rigos.ssh-hostkeys/v1"' \
    '"schema": "rigos.ssh-active-hostkeys/v1"' \
    'mode = "persistent"' \
    'mode = "ephemeral"' \
    'install_active_keyset' \
    'persistent_state_ready'
do
    grep -Fq "$required" "$hostkey_authority" \
        || die "SSH host-key authority contract is missing: $required"
done

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

for required in \
    'PRODUCTION_STATE = Path("/var/lib/rigos")' \
    'validate_msr_authority' \
    'randomx_msr_status_stale' \
    'randomx_msr_restore_state_missing' \
    'randomx_msr_authority_unsafe'
do
    grep -Fq "$required" "$miner_gate" || die "miner MSR safety contract is missing: $required"
done

printf 'RIGOS RandomX kernel MSR support: %s\n' "$msr_support"
printf 'RIGOS SSH host-key image verification passed\n'
printf 'RIGOS RandomX performance image verification passed: %s\n' "$image"
