#!/bin/bash
set -euo pipefail

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
[[ "$(sha256sum "$temporary/a/live/filesystem.squashfs" | cut -d' ' -f1)" == "$(jq -r .root_payload_sha256 "$manifest")" ]] || die 'root payload hash mismatch'
[[ "$(jq -r .schema "$temporary/a/image-layout.json")" == 'rigos.image-layout/v2' ]] || die 'embedded layout schema mismatch'
[[ "$(jq -r .partition_table "$temporary/a/image-layout.json")" == mbr ]] || die 'embedded layout table mismatch'
[[ "$(jq -r .final_state_partition "$temporary/a/image-layout.json")" == 4 ]] || die 'final state partition mismatch'
[[ "$(jq -r '.partitions[-1].label' "$temporary/a/image-layout.json")" == RIGOS_STATE_SEED ]] || die 'state seed is not final'
unsquashfs -no-progress -d "$temporary/root" "$temporary/a/live/filesystem.squashfs" \
  etc/rigos-release etc/os-release usr/lib/rigos/rigosd usr/lib/rigos/rigosctl \
  usr/lib/rigos/rigos-state-init usr/lib/rigos/xmrig usr/local/sbin/rigos-firstboot \
  usr/share/rigos >/dev/null
grep -Fqx "VERSION_ID=\"$image_version\"" "$temporary/root/etc/rigos-release" || die 'embedded release version mismatch'
grep -q 'NAME="RIGOS"' "$temporary/root/etc/os-release" || die 'embedded OS identity mismatch'
python3 -m py_compile "$temporary/root/usr/local/sbin/rigos-firstboot"
grep -Fq -- '"--output-fd", "1"' "$temporary/root/usr/local/sbin/rigos-firstboot" || die 'first boot output fd contract missing'
if grep -Fq 'stderr=subprocess.PIPE' "$temporary/root/usr/local/sbin/rigos-firstboot"; then die 'first boot hides whiptail UI'; fi
[[ "$(jq -r .modified "$temporary/root/usr/share/rigos/components/xmrig.json")" == false ]]
[[ "$(jq -r .upstream_donation_behavior "$temporary/root/usr/share/rigos/components/xmrig.json")" == applies ]]
[[ "$(jq -r .rigos_fee_percent "$temporary/root/usr/share/rigos/components/xmrig.json")" == 0 ]]
[[ "$(sha256sum "$temporary/root/usr/lib/rigos/xmrig" | cut -d' ' -f1)" == b20f39fc00d242e706b6c30367ad811c676e0575050a4ec2f30104b696944b49 ]]
[[ -f "$temporary/root/usr/share/rigos/licenses/XMRig-GPL-3.0.txt" ]]
[[ -f "$temporary/root/usr/share/rigos/THIRD_PARTY_NOTICES" ]]
if rg -n -i 'rigos.{0,20}(wallet|donation endpoint)|donation.{0,20}disabled|complete mining stack.{0,20}zero fee' "$temporary/root/usr/share/rigos"; then die 'forbidden miner fee claim or endpoint'; fi
printf 'RIGOS USB appliance verification passed: %s\n' "$image"
