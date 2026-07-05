#!/bin/bash
set -euo pipefail

die(){ printf 'verify-usb-appliance: %s\n' "$*" >&2; exit 1; }
[[ $# -eq 2 ]] || die 'usage: verify-usb-appliance.sh <image> <manifest>'
image="$(readlink -f "$1")"; manifest="$(readlink -f "$2")"
[[ -f "$image" && -f "$manifest" ]] || die 'image or manifest is missing'
sgdisk --verify "$image" | grep -q 'No problems found' || die 'GPT verification failed'
[[ "$(jq -r .schema "$manifest")" == 'rigos.image-build-manifest/v1' ]] || die 'manifest schema mismatch'
[[ "$(jq -r .artifact_sha256 "$manifest")" == "$(sha256sum "$image" | cut -d' ' -f1)" ]] || die 'image hash mismatch'
[[ "$(jq -r .source_commit "$manifest")" =~ ^[0-9a-f]{40}$ ]] || die 'invalid source commit'

p2="$(losetup --find --show --read-only --offset $((6144 * 512)) --sizelimit $((524288 * 512)) "$image")"
p3="$(losetup --find --show --read-only --offset $((530432 * 512)) --sizelimit $((2097152 * 512)) "$image")"
p4="$(losetup --find --show --read-only --offset $((2627584 * 512)) --sizelimit $((2097152 * 512)) "$image")"
p5="$(losetup --find --show --read-only --offset $((4724736 * 512)) --sizelimit $((524288 * 512)) "$image")"
temporary="$(mktemp -d)"
cleanup(){ set +e; mountpoint -q "$temporary/a" && umount "$temporary/a"; mountpoint -q "$temporary/b" && umount "$temporary/b"; losetup -d "$p5" "$p4" "$p3" "$p2" 2>/dev/null; rm -rf "$temporary"; }
trap cleanup EXIT
mkdir -p "$temporary/a" "$temporary/b" "$temporary/root"
mount -o ro "$p3" "$temporary/a"; mount -o ro "$p4" "$temporary/b"
for partition in '1 BIOS_BOOT' '2 EFI_SYSTEM' '3 RIGOS_ROOT_A' '4 RIGOS_ROOT_B' '5 RIGOS_STATE_SEED'; do
  number="${partition%% *}"; label="${partition#* }"
  sgdisk --info="$number" "$image" | grep -Fq "Partition name: '$label'" || die "partition $number label mismatch"
done
[[ "$(blkid -s LABEL -o value "$p5")" == RIGOS_STATE_SEED ]]
[[ "$(sha256sum "$p3" | cut -d' ' -f1)" == "$(jq -r .root_a_sha256 "$manifest")" ]] || die 'ROOT_A hash mismatch'
[[ "$(sha256sum "$p4" | cut -d' ' -f1)" == "$(jq -r .root_b_sha256 "$manifest")" ]] || die 'ROOT_B hash mismatch'
cmp "$temporary/a/live/filesystem.squashfs" "$temporary/b/live/filesystem.squashfs"
cmp "$temporary/a/image-layout.json" "$temporary/b/image-layout.json"
[[ "$(sha256sum "$temporary/a/live/filesystem.squashfs"|cut -d' ' -f1)" == "$(jq -r .root_payload_sha256 "$manifest")" ]]
[[ "$(jq -r .final_state_partition "$temporary/a/image-layout.json")" == 5 ]]
[[ "$(jq -r '.partitions[-1].label' "$temporary/a/image-layout.json")" == RIGOS_STATE_SEED ]]
unsquashfs -no-progress -d "$temporary/root" "$temporary/a/live/filesystem.squashfs" \
  etc/rigos-release etc/os-release usr/lib/rigos/rigosd usr/lib/rigos/rigosctl \
  usr/lib/rigos/rigos-state-init usr/lib/rigos/xmrig usr/share/rigos >/dev/null
grep -q 'VERSION_ID="0.0.4-alpha.1"' "$temporary/root/etc/rigos-release"
grep -q 'NAME="RIGOS"' "$temporary/root/etc/os-release"
[[ "$(jq -r .modified "$temporary/root/usr/share/rigos/components/xmrig.json")" == false ]]
[[ "$(jq -r .upstream_donation_behavior "$temporary/root/usr/share/rigos/components/xmrig.json")" == applies ]]
[[ "$(jq -r .rigos_fee_percent "$temporary/root/usr/share/rigos/components/xmrig.json")" == 0 ]]
[[ "$(sha256sum "$temporary/root/usr/lib/rigos/xmrig"|cut -d' ' -f1)" == b20f39fc00d242e706b6c30367ad811c676e0575050a4ec2f30104b696944b49 ]]
[[ -f "$temporary/root/usr/share/rigos/licenses/XMRig-GPL-3.0.txt" ]]
[[ -f "$temporary/root/usr/share/rigos/THIRD_PARTY_NOTICES" ]]
if rg -n -i 'rigos.{0,20}(wallet|donation endpoint)|donation.{0,20}disabled|complete mining stack.{0,20}zero fee' "$temporary/root/usr/share/rigos"; then die 'forbidden miner fee claim or endpoint'; fi
printf 'RIGOS USB appliance verification passed: %s\n' "$image"
