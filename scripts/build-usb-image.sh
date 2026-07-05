#!/bin/bash
set -euo pipefail

die() { printf 'build-usb-image: %s\n' "$*" >&2; exit 1; }
[[ "$(id -u)" -eq 0 ]] || die 'must run as root inside the image builder'
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
git -c safe.directory="$repo" diff --quiet || die 'tracked working tree is dirty'
git -c safe.directory="$repo" diff --cached --quiet || die 'Git index is dirty'
commit="$(git -c safe.directory="$repo" rev-parse HEAD)"
source_epoch="$(git -c safe.directory="$repo" log -1 --format=%ct)"

work=/work/rigos-appliance
source_root="$work/source"
live="$work/live"
rm -rf "$work"
mkdir -p "$source_root" "$live"
git -c safe.directory="$repo" archive HEAD | tar -x -C "$source_root"
# shellcheck disable=SC1091
source "$source_root/build/usb/version.env"

export RIGOS_BUILD_COMMIT="$commit"
export RIGOS_PRODUCT_VERSION RIGOS_IMAGE_ID RIGOS_IMAGE_VERSION RIGOS_IMAGE_CHANNEL
export RUSTUP_TOOLCHAIN=1.85.1-x86_64-unknown-linux-gnu
export RUSTFLAGS='-C target-cpu=x86-64'
export SOURCE_DATE_EPOCH="$source_epoch"
export CARGO_TARGET_DIR="$work/target"
cd "$source_root"
cargo build --release --locked -p rigosd -p rigos-state

mkdir -p "$live/config/package-lists" "$live/config/includes.chroot" \
  "$live/config/hooks/live" "$live/config/bootloaders"
cp build/usb/package-lists/* "$live/config/package-lists/"
cp -a build/usb/includes.chroot/. "$live/config/includes.chroot/"
cp build/usb/hooks/010-rigos.chroot "$live/config/hooks/live/010-rigos.hook.chroot"
cp -a build/usb/bootloaders/. "$live/config/bootloaders/"
install -D -m 0755 "$CARGO_TARGET_DIR/release/rigosd" "$live/config/includes.chroot/usr/lib/rigos/rigosd"
ln -s rigosd "$live/config/includes.chroot/usr/lib/rigos/rigosctl"
install -D -m 0755 "$CARGO_TARGET_DIR/release/rigos-state-init" "$live/config/includes.chroot/usr/lib/rigos/rigos-state-init"
ln -s /run/live/medium/image-layout.json "$live/config/includes.chroot/usr/lib/rigos/image-layout.json"

xmrig_archive="xmrig-${RIGOS_XMRIG_VERSION}-linux-static-x64.tar.gz"
xmrig_url="https://github.com/xmrig/xmrig/releases/download/v${RIGOS_XMRIG_VERSION}/${xmrig_archive}"
mkdir -p "$work/download"
curl --fail --location --proto '=https' --tlsv1.2 --output "$work/download/$xmrig_archive" "$xmrig_url"
printf '%s  %s\n' "$RIGOS_XMRIG_ARCHIVE_SHA256" "$work/download/$xmrig_archive" | sha256sum --check --strict
tar -tzf "$work/download/$xmrig_archive" | sort >"$work/xmrig.entries"
cat >"$work/xmrig.expected" <<EOF
xmrig-${RIGOS_XMRIG_VERSION}/
xmrig-${RIGOS_XMRIG_VERSION}/SHA256SUMS
xmrig-${RIGOS_XMRIG_VERSION}/config.json
xmrig-${RIGOS_XMRIG_VERSION}/xmrig
EOF
sort -o "$work/xmrig.expected" "$work/xmrig.expected"
cmp "$work/xmrig.expected" "$work/xmrig.entries" || die 'unexpected XMRig archive layout'
tar -xOf "$work/download/$xmrig_archive" "xmrig-${RIGOS_XMRIG_VERSION}/xmrig" >"$work/xmrig"
chmod 0755 "$work/xmrig"
printf '%s  %s\n' "$RIGOS_XMRIG_BINARY_SHA256" "$work/xmrig" | sha256sum --check --strict
install -D -m 0755 "$work/xmrig" "$live/config/includes.chroot/usr/lib/rigos/xmrig"

mkdir -p "$live/config/includes.chroot/usr/share/rigos/components" \
  "$live/config/includes.chroot/usr/share/rigos/licenses"
install -m 0644 build/usb/THIRD_PARTY_NOTICES "$live/config/includes.chroot/usr/share/rigos/THIRD_PARTY_NOTICES"
install -m 0644 /usr/share/common-licenses/GPL-3 "$live/config/includes.chroot/usr/share/rigos/licenses/XMRig-GPL-3.0.txt"
jq -n \
  --arg version "$RIGOS_XMRIG_VERSION" --arg artifact "$xmrig_archive" \
  --arg archive_sha "$RIGOS_XMRIG_ARCHIVE_SHA256" --arg binary_sha "$RIGOS_XMRIG_BINARY_SHA256" \
  '{schema:"rigos.component-provenance/v1",component:"xmrig",version:$version,source:"official-upstream-release",modified:false,architecture:"x86_64",artifact:$artifact,archive_sha256:$archive_sha,binary_sha256:$binary_sha,license:"GPL-3.0-or-later",upstream_donation_behavior:"applies",rigos_receives_donation:false,rigos_fee_percent:0}' \
  >"$live/config/includes.chroot/usr/share/rigos/components/xmrig.json"

build_date="$(date -u -d "@$source_epoch" +%Y%m%d)"
cat >"$live/config/includes.chroot/etc/rigos-release" <<EOF
NAME="RIGOS"
PRETTY_NAME="RIGOS ${RIGOS_IMAGE_VERSION} USB Appliance Preview"
ID=rigos
VERSION="${RIGOS_IMAGE_VERSION}"
VERSION_ID="${RIGOS_IMAGE_VERSION}"
VERSION_CODENAME="usb-appliance-preview"
BUILD_ID="${build_date}.${RIGOS_BUILD_ORDINAL}"
BUILD_COMMIT="${commit}"
IMAGE_ID="${RIGOS_IMAGE_ID}"
IMAGE_VERSION="${RIGOS_IMAGE_VERSION}"
IMAGE_CHANNEL="${RIGOS_IMAGE_CHANNEL}"
VARIANT="USB Appliance"
VARIANT_ID="usb"
ARCHITECTURE="x86_64"
BASE_ID="debian"
BASE_VERSION_ID="12"
RIGOS_SCHEMA="rigos.release/v1"
EOF
cat >"$live/config/includes.chroot/etc/os-release" <<EOF
NAME="RIGOS"
PRETTY_NAME="RIGOS ${RIGOS_IMAGE_VERSION}"
ID=rigos
ID_LIKE=debian
VERSION_ID="${RIGOS_IMAGE_VERSION}"
VERSION_CODENAME="usb-appliance-preview"
EOF

chmod 0755 "$live/config/hooks/live/"* "$live/config/includes.chroot/usr/local/sbin/rigos-firstboot"
find "$live/config/includes.chroot/etc/systemd/system" -type f -exec chmod 0644 {} +
find "$live/config/includes.chroot" -type f ! -path '*/usr/local/sbin/*' ! -path '*/usr/lib/rigos/rigos*' ! -path '*/usr/lib/rigos/xmrig' -exec chmod go-w {} +

cd "$live"
lb config noauto --mode debian --distribution bookworm --architectures amd64 \
  --binary-images iso-hybrid --bootloaders syslinux,grub-efi \
  --archive-areas 'main contrib non-free-firmware' --apt-recommends false \
  --debian-installer none --iso-application RIGOS --iso-publisher RIGOS \
  --iso-volume RIGOS_RECOVERY --bootappend-live 'boot=live components noautologin quiet rigos.stateless=1'
lb build

root_payload="$live/binary/live/filesystem.squashfs"
kernel="$live/binary/live/vmlinuz"
initrd="$live/binary/live/initrd.img"
[[ -f "$root_payload" && -f "$kernel" && -f "$initrd" ]] || die 'live root payload is incomplete'
root_payload_sha="$(sha256sum "$root_payload" | cut -d' ' -f1)"

layout="$work/image-layout.json"
jq -n --arg version "$RIGOS_IMAGE_VERSION" --arg commit "$commit" --arg payload "$root_payload_sha" \
  '{schema:"rigos.image-layout/v1",image_version:$version,image_id:"rigos-usb-amd64",partition_table:"gpt",disk_guid:"f578604e-1f8c-5543-a2d0-c16fe17ea7d8",logical_sector_size:512,minimum_media_size_bytes:8000000000,alignment_lba:2048,final_state_partition:5,build_commit:$commit,root_payload_sha256:$payload,partitions:[
  {number:1,label:"BIOS_BOOT",type_guid:"21686148-6449-6e6f-744e-656564454649",unique_guid:"b9c75afd-b347-5b75-848a-346309a170d1",start_lba:2048,minimum_size_lba:4096,filesystem:null},
  {number:2,label:"EFI_SYSTEM",type_guid:"c12a7328-f81f-11d2-ba4b-00a0c93ec93b",unique_guid:"f7f81abd-53a5-534d-bcc3-5cf59fd2a928",start_lba:6144,minimum_size_lba:524288,filesystem:"fat32"},
  {number:3,label:"RIGOS_ROOT_A",type_guid:"0b331da4-7c84-55c3-a328-a764d4641d1d",unique_guid:"0f22a113-890f-55d8-b444-01579fe225a0",start_lba:530432,minimum_size_lba:2097152,filesystem:"ext4"},
  {number:4,label:"RIGOS_ROOT_B",type_guid:"0b331da4-7c84-55c3-a328-a764d4641d1d",unique_guid:"97e3cb3a-0f93-5f4b-8e68-5f95f3734b46",start_lba:2627584,minimum_size_lba:2097152,filesystem:"ext4"},
  {number:5,label:"RIGOS_STATE_SEED",type_guid:"7ad8daed-eb61-5fbc-8fbd-d82f9e0b81ee",unique_guid:"56cb8d9e-20f8-5502-bfcd-3e457fe92bfe",start_lba:4724736,minimum_size_lba:524288,filesystem:"ext4"}]}' >"$layout"

image="$work/rigos-usb.img"
truncate -s $((5251072 * 512)) "$image"
sgdisk --clear --set-alignment=2048 --disk-guid=f578604e-1f8c-5543-a2d0-c16fe17ea7d8 \
  --new=1:2048:6143 --typecode=1:ef02 --change-name=1:BIOS_BOOT --partition-guid=1:b9c75afd-b347-5b75-848a-346309a170d1 \
  --new=2:6144:530431 --typecode=2:ef00 --change-name=2:EFI_SYSTEM --partition-guid=2:f7f81abd-53a5-534d-bcc3-5cf59fd2a928 \
  --new=3:530432:2627583 --typecode=3:0b331da4-7c84-55c3-a328-a764d4641d1d --change-name=3:RIGOS_ROOT_A --partition-guid=3:0f22a113-890f-55d8-b444-01579fe225a0 \
  --new=4:2627584:4724735 --typecode=4:0b331da4-7c84-55c3-a328-a764d4641d1d --change-name=4:RIGOS_ROOT_B --partition-guid=4:97e3cb3a-0f93-5f4b-8e68-5f95f3734b46 \
  --new=5:4724736:5249023 --typecode=5:7ad8daed-eb61-5fbc-8fbd-d82f9e0b81ee --change-name=5:RIGOS_STATE_SEED --partition-guid=5:56cb8d9e-20f8-5502-bfcd-3e457fe92bfe "$image"

loop="$(losetup --find --show "$image")"
p2="$(losetup --find --show --offset $((6144 * 512)) --sizelimit $((524288 * 512)) "$image")"
p3="$(losetup --find --show --offset $((530432 * 512)) --sizelimit $((2097152 * 512)) "$image")"
p4="$(losetup --find --show --offset $((2627584 * 512)) --sizelimit $((2097152 * 512)) "$image")"
p5="$(losetup --find --show --offset $((4724736 * 512)) --sizelimit $((524288 * 512)) "$image")"
cleanup(){ set +e; mountpoint -q "$work/mnt/state" && umount "$work/mnt/state"; mountpoint -q "$work/mnt/b" && umount "$work/mnt/b"; mountpoint -q "$work/mnt/a" && umount "$work/mnt/a"; mountpoint -q "$work/mnt/efi" && umount "$work/mnt/efi"; losetup -d "$p5" "$p4" "$p3" "$p2" "$loop" 2>/dev/null; }
trap cleanup EXIT
mkfs.vfat -F 32 -n EFI_SYSTEM "$p2"
mkfs.ext4 -q -F -L RIGOS_ROOT_A -U 065b5c7f-076a-50dd-92e4-a600a5c6682f -m 0 "$p3"
mkfs.ext4 -q -F -L RIGOS_ROOT_B -U f6285e01-c386-528f-bf33-910c744dd8ba -m 0 "$p4"
mkfs.ext4 -q -F -L RIGOS_STATE_SEED -U dc450e72-daa4-5b82-8d1b-0ae6b11607f9 -m 0 "$p5"
mkdir -p "$work/mnt/efi" "$work/mnt/a" "$work/mnt/b" "$work/mnt/state"
mount "$p2" "$work/mnt/efi"; mount "$p3" "$work/mnt/a"; mount "$p4" "$work/mnt/b"; mount "$p5" "$work/mnt/state"
for slot in a b; do
  mkdir -p "$work/mnt/$slot/live" "$work/mnt/$slot/boot/grub"
  install -m 0644 "$root_payload" "$work/mnt/$slot/live/filesystem.squashfs"
  install -m 0644 "$kernel" "$work/mnt/$slot/live/vmlinuz"
  install -m 0644 "$initrd" "$work/mnt/$slot/live/initrd.img"
  install -m 0644 "$layout" "$work/mnt/$slot/image-layout.json"
done
grub-install --target=i386-pc --boot-directory="$work/mnt/a/boot" --no-floppy "$loop"
grub-install --target=x86_64-efi --efi-directory="$work/mnt/efi" --boot-directory="$work/mnt/a/boot" --removable --no-nvram
cp -a "$work/mnt/a/boot/grub/." "$work/mnt/b/boot/grub/"
cat >"$work/grub.cfg" <<'EOF'
set timeout=5
set default=0
insmod all_video
insmod part_gpt
insmod ext2

menuentry 'RIGOS 0.0.4-alpha.1' {
    search --no-floppy --part-label RIGOS_ROOT_A --set=root
    linux /live/vmlinuz boot=live components live-media=/dev/disk/by-partlabel/RIGOS_ROOT_A live-media-path=/live ro noeject noautologin console=tty0 console=ttyS0,115200n8
    initrd /live/initrd.img
}
menuentry 'RIGOS 0.0.4-alpha.1 -- safe mode' {
    search --no-floppy --part-label RIGOS_ROOT_A --set=root
    linux /live/vmlinuz boot=live components live-media=/dev/disk/by-partlabel/RIGOS_ROOT_A live-media-path=/live ro noeject noautologin nomodeset noapic noacpi console=tty0 console=ttyS0,115200n8
    initrd /live/initrd.img
}
menuentry 'RIGOS ROOT_B fallback' {
    search --no-floppy --part-label RIGOS_ROOT_B --set=root
    linux /live/vmlinuz boot=live components live-media=/dev/disk/by-partlabel/RIGOS_ROOT_B live-media-path=/live ro noeject noautologin console=tty0 console=ttyS0,115200n8
    initrd /live/initrd.img
}
EOF
install -m 0644 "$work/grub.cfg" "$work/mnt/a/boot/grub/grub.cfg"
install -m 0644 "$work/grub.cfg" "$work/mnt/b/boot/grub/grub.cfg"
sync
root_a_sha="$(sha256sum "$p3" | cut -d' ' -f1)"; root_b_sha="$(sha256sum "$p4" | cut -d' ' -f1)"
umount "$work/mnt/state" "$work/mnt/b" "$work/mnt/a" "$work/mnt/efi"
sgdisk --attributes=3:set:60 --attributes=4:set:60 "$image"
sync; losetup -d "$p5" "$p4" "$p3" "$p2" "$loop"; trap - EXIT

output="$repo/dist/usb"
rm -rf "$output"; mkdir -p "$output"
image_name="rigos-usb-amd64-${RIGOS_IMAGE_VERSION}.img"
recovery_name="rigos-recovery-amd64-${RIGOS_IMAGE_VERSION}.iso"
install -m 0644 "$image" "$output/$image_name"
install -m 0644 "$live/live-image-amd64.hybrid.iso" "$output/$recovery_name"
image_sha="$(sha256sum "$output/$image_name" | cut -d' ' -f1)"
recovery_sha="$(sha256sum "$output/$recovery_name" | cut -d' ' -f1)"
printf '%s  %s\n' "$image_sha" "$image_name" >"$output/$image_name.sha256"
printf '%s  %s\n' "$recovery_sha" "$recovery_name" >"$output/$recovery_name.sha256"

kernel_version="$(basename "$(find "$live/chroot/boot" -maxdepth 1 -name 'vmlinuz-*' | sort | tail -1)" | sed 's/^vmlinuz-//')"
jq -n --arg version "$RIGOS_IMAGE_VERSION" --arg channel "$RIGOS_IMAGE_CHANNEL" --arg commit "$commit" \
  --arg artifact "$image_name" --arg sha "$image_sha" --argjson size "$(stat -c %s "$output/$image_name")" \
  --arg root_a "$root_a_sha" --arg root_b "$root_b_sha" --arg payload "$root_payload_sha" \
  --arg kernel "$kernel_version" --argjson epoch "$source_epoch" --slurpfile layout "$layout" \
  --slurpfile xmrig "$live/config/includes.chroot/usr/share/rigos/components/xmrig.json" \
  '{schema:"rigos.image-build-manifest/v1",product:"RIGOS",product_version:$version,image_id:"rigos-usb-amd64",image_version:$version,image_channel:$channel,source_commit:$commit,source_date_epoch:$epoch,target:"x86_64-unknown-linux-gnu",base:"Debian GNU/Linux 12",kernel:$kernel,artifact:$artifact,artifact_sha256:$sha,artifact_size_bytes:$size,root_a_sha256:$root_a,root_b_sha256:$root_b,root_payload_sha256:$payload,layout:$layout[0],components:[$xmrig[0]],tools:{rustc:"1.85.1",live_build:"20230502",grub:"2.06"}}' \
  >"$output/rigos-usb-amd64-${RIGOS_IMAGE_VERSION}.build-manifest.json"

"$source_root/scripts/verify-usb-appliance.sh" "$output/$image_name" "$output/rigos-usb-amd64-${RIGOS_IMAGE_VERSION}.build-manifest.json"
"$source_root/scripts/verify-usb-image.sh" "$output/$recovery_name"
printf 'RIGOS appliance: %s\nRecovery ISO: %s\n' "$output/$image_name" "$output/$recovery_name"
