#!/bin/bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
die(){ printf 'verify-usb-appliance: %s\n' "$*" >&2; exit 1; }
[[ $# -eq 2 ]] || die 'usage: verify-usb-appliance.sh <image> <manifest>'
image="$(readlink -f "$1")"; manifest="$(readlink -f "$2")"
[[ -f "$image" && -f "$manifest" ]] || die 'image or manifest is missing'
[[ "$(jq -r .schema "$manifest")" == 'rigos.image-build-manifest/v2' ]] || die 'manifest schema mismatch'
[[ "$(jq -r .artifact_sha256 "$manifest")" == "$(sha256sum "$image" | cut -d' ' -f1)" ]] || die 'image hash mismatch'
[[ "$(jq -r .source_commit "$manifest")" =~ ^[0-9a-f]{40}$ ]] || die 'invalid source commit'
[[ "$(jq -r .layout.schema "$manifest")" == 'rigos.image-layout/v2' ]] || die 'layout schema mismatch'
[[ "$(jq -r .layout.partition_table "$manifest")" == mbr ]] || die 'partition table contract mismatch'
[[ "$(jq -r .layout.disk_guid "$manifest")" == '0x5249474f' ]] || die 'disk signature contract mismatch'
image_version="$(jq -r .image_version "$manifest")"
[[ -n "$image_version" && "$image_version" != null ]] || die 'image version is missing'

signature="$(od -An -tx1 -j510 -N2 "$image" | tr -d ' \n')"
[[ "$signature" == 55aa ]] || die 'MBR signature is missing'
contains_nonzero(){
  od -An -v -tu1 | awk '{ for (i = 1; i <= NF; i++) if ($i != 0) found = 1 } END { exit found ? 0 : 1 }'
}
dd if="$image" bs=1 count=446 status=none | contains_nonzero || die 'MBR boot code is empty'
dd if="$image" bs=512 skip=1 count=2047 status=none | contains_nonzero || die 'GRUB embedding gap is empty'

partition_json="$(sfdisk --json "$image")"
[[ "$(jq -r .partitiontable.label <<<"$partition_json")" == dos ]] || die 'DOS partition table is missing'
[[ "$(jq -r .partitiontable.id <<<"$partition_json" | tr '[:lower:]' '[:upper:]')" == 0X5249474F ]] || die 'MBR disk signature mismatch'
[[ "$(jq '.partitiontable.partitions | length' <<<"$partition_json")" -eq 4 ]] || die 'unexpected partition count'

check_partition(){
  local index="$1" start="$2" size="$3" type="$4" bootable="$5" base observed_type
  base=".partitiontable.partitions[$((index - 1))]"
  [[ "$(jq -r "$base.start" <<<"$partition_json")" -eq "$start" ]] || die "partition $index start mismatch"
  [[ "$(jq -r "$base.size" <<<"$partition_json")" -eq "$size" ]] || die "partition $index size mismatch"
  observed_type="$(jq -r "$base.type" <<<"$partition_json" | tr '[:upper:]' '[:lower:]')"
  observed_type="${observed_type#0x}"
  [[ "$observed_type" == "$type" ]] || die "partition $index type mismatch"
  if [[ "$bootable" == yes ]]; then
    [[ "$(jq -r "$base.bootable // false" <<<"$partition_json")" == true ]] || die "partition $index is not active"
  else
    [[ "$(jq -r "$base.bootable // false" <<<"$partition_json")" == false ]] || die "partition $index is unexpectedly active"
  fi
}
check_partition 1 2048 524288 c yes
check_partition 2 526336 2097152 83 no
check_partition 3 2623488 2097152 83 no
check_partition 4 4720640 524288 83 no

p1="$(losetup --find --show --read-only --offset $((2048 * 512)) --sizelimit $((524288 * 512)) "$image")"
p2="$(losetup --find --show --read-only --offset $((526336 * 512)) --sizelimit $((2097152 * 512)) "$image")"
p3="$(losetup --find --show --read-only --offset $((2623488 * 512)) --sizelimit $((2097152 * 512)) "$image")"
p4="$(losetup --find --show --read-only --offset $((4720640 * 512)) --sizelimit $((524288 * 512)) "$image")"
temporary="$(mktemp -d)"
cleanup(){ set +e; mountpoint -q "$temporary/efi" && umount "$temporary/efi"; mountpoint -q "$temporary/a" && umount "$temporary/a"; mountpoint -q "$temporary/b" && umount "$temporary/b"; losetup -d "$p4" "$p3" "$p2" "$p1" 2>/dev/null; rm -rf "$temporary"; }
trap cleanup EXIT
mkdir -p "$temporary/efi" "$temporary/a" "$temporary/b" "$temporary/root"
mount -o ro "$p1" "$temporary/efi"
mount -o ro "$p2" "$temporary/a"
mount -o ro "$p3" "$temporary/b"

[[ "$(blkid -s LABEL -o value "$p1")" == EFI_SYSTEM ]] || die 'EFI filesystem label mismatch'
[[ "$(blkid -s LABEL -o value "$p2")" == RIGOS_ROOT_A ]] || die 'ROOT_A filesystem label mismatch'
[[ "$(blkid -s LABEL -o value "$p3")" == RIGOS_ROOT_B ]] || die 'ROOT_B filesystem label mismatch'
[[ "$(blkid -s LABEL -o value "$p4")" == RIGOS_STATE_SEED ]] || die 'state seed filesystem label mismatch'
[[ -f "$temporary/efi/EFI/BOOT/BOOTX64.EFI" ]] || die 'removable UEFI loader is missing'
[[ "$(sha256sum "$p2" | cut -d' ' -f1)" == "$(jq -r .root_a_sha256 "$manifest")" ]] || die 'ROOT_A hash mismatch'
[[ "$(sha256sum "$p3" | cut -d' ' -f1)" == "$(jq -r .root_b_sha256 "$manifest")" ]] || die 'ROOT_B hash mismatch'
cmp "$temporary/a/live/filesystem.squashfs" "$temporary/b/live/filesystem.squashfs"
cmp "$temporary/a/image-layout.json" "$temporary/b/image-layout.json"
squashfs="$temporary/a/live/filesystem.squashfs"
[[ "$(sha256sum "$squashfs" | cut -d' ' -f1)" == "$(jq -r .root_payload_sha256 "$manifest")" ]] || die 'root payload hash mismatch'
[[ "$(jq -r .schema "$temporary/a/image-layout.json")" == 'rigos.image-layout/v2' ]] || die 'embedded layout schema mismatch'
[[ "$(jq -r .partition_table "$temporary/a/image-layout.json")" == mbr ]] || die 'embedded layout table mismatch'
[[ "$(jq -r .final_state_partition "$temporary/a/image-layout.json")" == 4 ]] || die 'final state partition mismatch'
[[ "$(jq -r '.partitions[-1].label' "$temporary/a/image-layout.json")" == RIGOS_STATE_SEED ]] || die 'state seed is not final'

listing="$temporary/squashfs.list"
unsquashfs -ll "$squashfs" >"$listing"
if awk '{print $NF}' "$listing" | grep -E '^squashfs-root/etc/ssh/ssh_host_.*_key(\.pub)?$' >/dev/null; then
  die 'appliance image contains a baked SSH host key'
fi

unsquashfs -no-progress -d "$temporary/root" "$squashfs" \
  etc/rigos-release etc/os-release \
  etc/ssh/sshd_config.d/01-rigos-hostkeys.conf \
  usr/lib/tmpfiles.d/rigos.conf \
  etc/systemd/system/rigos-state.service \
  etc/systemd/system/rigos-state-ready.service \
  etc/systemd/system/rigos-recovery-access.service \
  etc/systemd/system/rigos-ssh-hostkeys.service \
  etc/systemd/system/ssh.service.d/rigos-observe.conf \
  etc/systemd/system/multi-user.target.wants/rigos-ssh-hostkeys.service \
  etc/systemd/system/rigos-firstboot.service \
  etc/systemd/system/rigos-hugepages.service \
  etc/systemd/system/rigos-miner.service \
  etc/systemd/system/rigos-miner.service.d/runtime-render.conf \
  etc/systemd/system/rigos-miner.service.d/stability.conf \
  etc/systemd/system/rigos-miner-health.service \
  etc/systemd/system/rigos-miner-health.timer \
  etc/systemd/system/rigos-runtime-render.service \
  etc/systemd/system/rigos-profile-apply.service \
  usr/bin/jq usr/bin/python3 usr/bin/python3.11 usr/bin/findmnt usr/bin/ssh-keygen \
  usr/lib/rigos/rigosd usr/lib/rigos/rigosctl \
  usr/lib/rigos/lsblk-compat usr/lib/rigos/rigos-state-init usr/lib/rigos/rigos-state-ready usr/lib/rigos/rigos-config usr/lib/rigos/rigos-performance usr/lib/rigos/rigos-lifecycle-cycles usr/lib/rigos/rigos-miner-gate usr/lib/rigos/rigos-miner-health usr/lib/rigos/rigos-runtime-render usr/lib/rigos/rigos-runtime-publish usr/lib/rigos/rigos-runtime-authority usr/lib/rigos/rigos-runtime-gate usr/lib/rigos/rigos-ssh-hostkeys usr/lib/rigos/xmrig \
  usr/local/bin/rigosd usr/local/bin/rigosctl \
  usr/local/sbin/rigosctl usr/local/sbin/rigos-firstboot usr/local/sbin/rigos-recovery-access usr/local/sbin/rigos-state-orchestrate \
  usr/share/rigos >/dev/null
grep -Fqx "VERSION_ID=\"$image_version\"" "$temporary/root/etc/rigos-release" || die 'embedded release version mismatch'
grep -q 'NAME="RIGOS"' "$temporary/root/etc/os-release" || die 'embedded OS identity mismatch'
[[ -x "$temporary/root/usr/bin/jq" ]] || die 'jq runtime dependency is missing from the appliance'
[[ -x "$temporary/root/usr/bin/findmnt" ]] || die 'findmnt runtime dependency is missing from the appliance'
[[ -x "$temporary/root/usr/bin/ssh-keygen" ]] || die 'ssh-keygen runtime dependency is missing from the appliance'
python3 -m py_compile "$temporary/root/usr/local/sbin/rigos-firstboot"
python3 -m py_compile "$temporary/root/usr/local/sbin/rigos-recovery-access"
python3 -m py_compile "$temporary/root/usr/local/sbin/rigos-state-orchestrate"
python3 -m py_compile "$temporary/root/usr/lib/rigos/rigos-miner-gate"
python3 -m py_compile "$temporary/root/usr/lib/rigos/rigos-miner-health"
python3 -m py_compile "$temporary/root/usr/lib/rigos/rigos-ssh-hostkeys"
sh -n "$temporary/root/usr/lib/rigos/rigos-runtime-publish"
sh -n "$temporary/root/usr/lib/rigos/rigos-runtime-authority"
python3 "$script_dir/verify-systemd-ordering.py" "$temporary/root/etc/systemd/system"
rigosctl_path="$(PATH="$temporary/root/usr/local/sbin:$temporary/root/usr/bin" command -v rigosctl)"
[[ "$rigosctl_path" == "$temporary/root/usr/local/sbin/rigosctl" && -x "$rigosctl_path" ]] || die 'rigosctl is not executable in the appliance PATH'
grep -Fq 'systemd-tmpfiles --create /usr/lib/tmpfiles.d/rigos.conf' "$temporary/root/etc/systemd/system/rigos-state.service" || die 'state runtime tmpfiles setup is missing'
grep -Fq 'ExecStart=/usr/local/sbin/rigos-state-orchestrate' "$temporary/root/etc/systemd/system/rigos-state.service" || die 'state resume orchestrator is not wired'
grep -Fqx 'd /run/rigos 0755 root root -' "$temporary/root/usr/lib/tmpfiles.d/rigos.conf" || die 'shared runtime directory contract is missing'
if rg -q '^RuntimeDirectory=rigos$' "$temporary/root/etc/systemd/system"; then die 'a service owns the shared runtime directory'; fi
if grep -Eq '^(ExecStartPre|Environment=PATH)=.*compat-bin' "$temporary/root/etc/systemd/system/rigos-state.service"; then die 'state unit executes compatibility code from the runtime directory'; fi
[[ -f "$temporary/root/usr/lib/rigos/lsblk-compat" ]] || die 'state lsblk compatibility wrapper is missing'
[[ -x "$temporary/root/usr/bin/python3" ]] || die 'Python runtime for state compatibility is missing'
strings "$temporary/root/usr/lib/rigos/rigos-state-init" | grep -F '/usr/bin/python3' >/dev/null || die 'state initializer does not use the absolute Python runtime'
strings "$temporary/root/usr/lib/rigos/rigos-state-init" | grep -F '/usr/lib/rigos/lsblk-compat' >/dev/null || die 'state initializer does not use the packaged compatibility wrapper'
strings "$temporary/root/usr/lib/rigos/rigos-state-init" | grep -F -- '--tree' >/dev/null || die 'state initializer does not require hierarchical lsblk output'
if strings "$temporary/root/usr/lib/rigos/rigos-state-init" | grep -F '/run/rigos/compat-bin/lsblk' >/dev/null; then die 'state initializer executes compatibility code from the runtime directory'; fi
grep -Fq 'ExecStart=/usr/lib/rigos/rigos-state-ready' "$temporary/root/etc/systemd/system/rigos-state-ready.service" || die 'state readiness verifier is not wired'
if grep -Fq 'Wants=rigos-recovery-access.service' "$temporary/root/etc/systemd/system/rigos-state-ready.service"; then die 'state readiness retriggers interactive recovery access'; fi
grep -Fq 'Requires=rigos-state-ready.service' "$temporary/root/etc/systemd/system/rigos-profile-apply.service" || die 'profile apply bypasses state readiness'

hostkey_service="$temporary/root/etc/systemd/system/rigos-ssh-hostkeys.service"
hostkey_policy="$temporary/root/etc/ssh/sshd_config.d/01-rigos-hostkeys.conf"
ssh_dropin="$temporary/root/etc/systemd/system/ssh.service.d/rigos-observe.conf"
hostkey_authority="$temporary/root/usr/lib/rigos/rigos-ssh-hostkeys"
[[ -x "$hostkey_authority" ]] || die 'persistent SSH host-key authority is missing or not executable'
[[ -L "$temporary/root/etc/systemd/system/multi-user.target.wants/rigos-ssh-hostkeys.service" ]] || die 'persistent SSH host-key service is not enabled'
for required in \
  'After=rigos-state-ready.service' \
  'Requires=rigos-state-ready.service' \
  'Before=ssh.service' \
  'ExecStart=/usr/lib/rigos/rigos-ssh-hostkeys' \
  'ReadWritePaths=/var/lib/rigos /run/rigos'
do
  grep -Fqx "$required" "$hostkey_service" || die "persistent SSH host-key service contract is missing: $required"
done
grep -Fqx 'HostKey /var/lib/rigos/system/ssh-hostkeys/ssh_host_ed25519_key' "$hostkey_policy" || die 'sshd persistent HostKey policy is missing'
grep -Fqx 'Requires=rigos-ssh-hostkeys.service' "$ssh_dropin" || die 'ssh.service does not require persistent host identity'
grep -Fqx 'After=rigos-recovery-access.service rigos-ssh-hostkeys.service' "$ssh_dropin" || die 'ssh.service ordering bypasses persistent host identity'
for required in \
  'STATE = Path("/var/lib/rigos")' \
  'KEYS = SYSTEM / "ssh-hostkeys"' \
  '"schema": "rigos.ssh-hostkeys/v1"' \
  'os.rename(temporary, KEYS)' \
  '"persistent SSH host identity exists without a valid manifest"'
do
  grep -Fq "$required" "$hostkey_authority" || die "persistent SSH host-key authority contract is missing: $required"
done

[[ -x "$temporary/root/usr/lib/rigos/rigos-performance" ]] || die 'performance authority is missing or not executable'
grep -Fq 'After=rigos-state-ready.service rigos-profile-apply.service' "$temporary/root/etc/systemd/system/rigos-hugepages.service" || die 'huge page authority ordering is missing'
grep -Fq 'Before=rigos-miner.service' "$temporary/root/etc/systemd/system/rigos-hugepages.service" || die 'huge page authority is not ordered before miner'
grep -Fq 'Requires=rigos-state-ready.service rigos-profile-apply.service' "$temporary/root/etc/systemd/system/rigos-hugepages.service" || die 'huge page authority dependencies are missing'
grep -Fq 'Requires=rigos-state-ready.service rigos-hugepages.service' "$temporary/root/etc/systemd/system/rigos-miner.service" || die 'miner does not require huge page authority'
strings "$temporary/root/usr/lib/rigos/rigos-performance" | grep -F '/proc/sys/vm/nr_hugepages' >/dev/null || die 'performance authority does not use direct kernel huge page control'
strings "$temporary/root/usr/lib/rigos/rigos-performance" | grep -F 'rigos.performance-status/v1' >/dev/null || die 'performance authority status contract is missing'
if strings "$temporary/root/usr/lib/rigos/rigos-performance" | grep -F 'sysctl' >/dev/null; then die 'performance authority shells out to sysctl'; fi
if strings "$temporary/root/usr/lib/rigos/rigos-performance" | grep -Ei '(/dev/sd|/dev/nvme|cpu model)' >/dev/null; then die 'performance authority contains hardware-name or internal-disk targeting'; fi
grep -Fq 'ExecCondition=/usr/lib/rigos/rigos-miner-gate' "$temporary/root/etc/systemd/system/rigos-miner.service" || die 'miner safety gate is missing'
[[ -x "$temporary/root/usr/lib/rigos/rigos-runtime-render" ]] || die 'legacy runtime renderer is missing'
[[ -x "$temporary/root/usr/lib/rigos/rigos-runtime-publish" ]] || die 'runtime allowlist publisher is missing'
[[ -x "$temporary/root/usr/lib/rigos/rigos-runtime-authority" ]] || die 'serialized runtime authority is missing'
grep -Fq 'ExecStart=/usr/lib/rigos/rigos-runtime-authority' "$temporary/root/etc/systemd/system/rigos-runtime-render.service" || die 'serialized runtime authority is not wired'
grep -Fq 'ExecCondition=+/usr/lib/rigos/rigos-runtime-authority' "$temporary/root/etc/systemd/system/rigos-miner.service.d/runtime-render.conf" || die 'miner does not serialize runtime publication before start'
grep -Fq 'ExecCondition=/usr/lib/rigos/rigos-runtime-gate' "$temporary/root/etc/systemd/system/rigos-miner.service.d/runtime-render.conf" || die 'runtime gate is missing from miner override'
grep -Fq 'ExecStart=/usr/lib/rigos/xmrig -c /run/rigos/xmrig.json' "$temporary/root/etc/systemd/system/rigos-miner.service.d/runtime-render.conf" || die 'managed miner does not use the short private runtime config option'
grep -Fq 'flock -x -w 30' "$temporary/root/usr/lib/rigos/rigos-runtime-authority" || die 'runtime publication lock is missing'
grep -Fq 'jq_bin=${RIGOS_JQ:-/usr/bin/jq}' "$temporary/root/usr/lib/rigos/rigos-runtime-publish" || die 'runtime publisher does not use the absolute jq dependency'
grep -Fq 'construction: "allowlist"' "$temporary/root/usr/lib/rigos/rigos-runtime-publish" || die 'public runtime allowlist marker is missing'
grep -Fq '.render-stage.XXXXXX' "$temporary/root/usr/lib/rigos/rigos-runtime-publish" || die 'private runtime staging directory is missing'
grep -Fq -- '--xmrig-config /run/rigos/xmrig-public.json' "$temporary/root/usr/local/bin/rigosd" || die 'rigosd does not default to the public runtime view'
grep -Fq -- '--xmrig-config /run/rigos/xmrig-public.json' "$temporary/root/usr/local/bin/rigosctl" || die 'rigosctl does not default to the public runtime view'
[[ -x "$temporary/root/usr/lib/rigos/rigos-miner-health" ]] || die 'miner health observer is missing'
grep -Fq 'OnUnitActiveSec=1min' "$temporary/root/etc/systemd/system/rigos-miner-health.timer" || die 'miner health timer cadence is missing'
grep -Fq 'Restart=on-failure' "$temporary/root/etc/systemd/system/rigos-miner.service.d/stability.conf" || die 'bounded miner restart policy is missing'
grep -Fq 'StartLimitBurst=5' "$temporary/root/etc/systemd/system/rigos-miner.service.d/stability.conf" || die 'miner crash-loop ceiling is missing'
[[ -f "$temporary/root/etc/systemd/system/rigos-profile-apply.service" ]] || die 'profile apply service is missing'
grep -Fq 'ExecCondition=/usr/lib/rigos/rigos-config needs-activation' "$temporary/root/etc/systemd/system/rigos-firstboot.service" || die 'first boot activation gate is missing'
grep -Fq 'ExecStart=/usr/local/sbin/rigos-recovery-access' "$temporary/root/etc/systemd/system/rigos-recovery-access.service" || die 'local recovery access phase is missing'
grep -Fq 'CREDENTIAL_DIRECTORY = STATE / "recovery"' "$temporary/root/usr/local/sbin/rigos-recovery-access" || die 'persistent recovery credential store is missing'
grep -Fq '["/usr/sbin/chpasswd", "--encrypted"]' "$temporary/root/usr/local/sbin/rigos-recovery-access" || die 'encrypted credential restore is missing'
[[ -x "$temporary/root/usr/lib/rigos/rigos-lifecycle-cycles" ]] || die 'booted lifecycle cycle test is missing'
if rg -q -- '--output-fd' "$temporary/root/usr/local/sbin/rigos-firstboot"; then die 'first boot rewires the whiptail screen stream'; fi
grep -Fq 'stderr=subprocess.PIPE' "$temporary/root/usr/local/sbin/rigos-firstboot" || die 'first boot stderr capture is missing'
grep -Fq 'return result.stderr.strip()' "$temporary/root/usr/local/sbin/rigos-firstboot" || die 'first boot value stream mismatch'
[[ "$(jq -r .modified "$temporary/root/usr/share/rigos/components/xmrig.json")" == false ]]
[[ "$(jq -r .upstream_donation_behavior "$temporary/root/usr/share/rigos/components/xmrig.json")" == applies ]]
[[ "$(jq -r .rigos_fee_percent "$temporary/root/usr/share/rigos/components/xmrig.json")" == 0 ]]
[[ "$(sha256sum "$temporary/root/usr/lib/rigos/xmrig" | cut -d' ' -f1)" == b20f39fc00d242e706b6c30367ad811c676e0575050a4ec2f30104b696944b49 ]]
[[ -f "$temporary/root/usr/share/rigos/licenses/XMRig-GPL-3.0.txt" ]]
[[ -f "$temporary/root/usr/share/rigos/THIRD_PARTY_NOTICES" ]]
if rg -n -i 'rigos.{0,20}(wallet|donation endpoint)|donation.{0,20}disabled|complete mining stack.{0,20}zero fee' "$temporary/root/usr/share/rigos"; then die 'forbidden miner fee claim or endpoint'; fi
printf 'RIGOS USB appliance verification passed: %s\n' "$image"
