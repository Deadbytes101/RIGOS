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
cargo build --release --locked -p rigosd -p rigos-state -p rigos-config -p rigos-performance

mkdir -p "$live/config/package-lists" "$live/config/includes.chroot" \
  "$live/config/hooks/live" "$live/config/bootloaders"
cp build/usb/package-lists/* "$live/config/package-lists/"
cp -a build/usb/includes.chroot/. "$live/config/includes.chroot/"
cp build/usb/hooks/010-rigos.chroot "$live/config/hooks/live/010-rigos.hook.chroot"
cp -a build/usb/bootloaders/. "$live/config/bootloaders/"
install -D -m 0755 "$CARGO_TARGET_DIR/release/rigosd" "$live/config/includes.chroot/usr/lib/rigos/rigosd"
ln -s rigosd "$live/config/includes.chroot/usr/lib/rigos/rigosctl"
mkdir -p "$live/config/includes.chroot/usr/local/sbin"
ln -s ../../lib/rigos/rigosctl "$live/config/includes.chroot/usr/local/sbin/rigosctl"
install -D -m 0755 "$CARGO_TARGET_DIR/release/rigos-state-init" "$live/config/includes.chroot/usr/lib/rigos/rigos-state-init"
install -D -m 0755 "$CARGO_TARGET_DIR/release/rigos-state-ready" "$live/config/includes.chroot/usr/lib/rigos/rigos-state-ready"
install -D -m 0755 "$CARGO_TARGET_DIR/release/rigos-config" "$live/config/includes.chroot/usr/lib/rigos/rigos-config"
install -D -m 0755 "$CARGO_TARGET_DIR/release/rigos-performance" "$live/config/includes.chroot/usr/lib/rigos/rigos-performance"
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

chmod 0755 "$live/config/hooks/live/"* "$live/config/includes.chroot/usr/local/sbin/rigos-firstboot" "$live/config/includes.chroot/usr/local/sbin/rigos-recovery-access"
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
  '{schema:"rigos.image-layout/v2",image_version:$version,image_id:"rigos-usb-amd64",partition_table:"mbr",disk_guid:"0x5249474f",logical_sector_size:512,minimum_media_size_bytes:8000000000,alignment_lba:2048,final_state_partition:4,build_commit:$commit,root_payload_sha256:$payload,partitions:[
  {number:1,label:"EFI_SYSTEM",type_guid:"0x0c",unique_guid:"5249474f-01",start_lba:2048,minimum_size_lba:524288,filesystem:"fat32"},
  {number:2,label:"RIGOS_ROOT_A",type_guid:"0x83",unique_guid:"5249474f-02",start_lba:526336,minimum_size_lba:2097152,filesystem:"ext4"},
  {number:3,label:"RIGOS_ROOT_B",type_guid:"0x83",unique_guid:"5249474f-03",start_lba:2623488,minimum_size_lba:2097152,filesystem:"ext4"},
  {number:4,label:"RIGOS_STATE_SEED",type_guid:"0x83",unique_guid:"5249474f-04",start_lba:4720640,minimum_size_lba:524288,filesystem:"ext4"}]}' >"$layout"

image="$work/rigos-usb.img"
truncate -s $((5244928 * 512)) "$image"
sfdisk "$image" <<'EOF'
label: dos
label-id: 0x5249474f
unit: sectors
sector-size: 512

start=2048, size=524288, type=c, bootable
start=526336, size=2097152, type=83
start=2623488, size=2097152, type=83
start=4720640, size=524288, type=83
EOF

loop="$(losetup --find --show --partscan "$image")"
loop_name="$(basename "$loop")"
for number in 1 2 3 4; do
  node="${loop}p${number}"
  device_number="$(cat "/sys/class/block/${loop_name}p${number}/dev")"
  [[ ! -e "$node" ]] || die "partition node already exists: $node"
  mknod "$node" b "${device_number%:*}" "${device_number#*:}"
done
p1="${loop}p1"; p2="${loop}p2"; p3="${loop}p3"; p4="${loop}p4"
cleanup(){ set +e; mountpoint -q "$work/mnt/state" && umount "$work/mnt/state"; mountpoint -q "$work/mnt/b" && umount "$work/mnt/b"; mountpoint -q "$work/mnt/a" && umount "$work/mnt/a"; mountpoint -q "$work/mnt/efi" && umount "$work/mnt/efi"; losetup -d "$loop" 2>/dev/null; rm -f "${loop}p"{1,2,3,4}; }
trap cleanup EXIT
mkfs.vfat -F 32 -n EFI_SYSTEM "$p1"
mkfs.ext4 -q -F -L RIGOS_ROOT_A -U 065b5c7f-076a-50dd-92e4-a600a5c6682f -m 0 "$p2"
mkfs.ext4 -q -F -L RIGOS_ROOT_B -U f6285e01-c386-528f-bf33-910c744dd8ba -m 0 "$p3"
mkfs.ext4 -q -F -L RIGOS_STATE_SEED -U dc450e72-daa4-5b82-8d1b-0ae6b11607f9 -m 0 "$p4"
mkdir -p "$work/mnt/efi" "$work/mnt/a" "$work/mnt/b" "$work/mnt/state"
mount "$p1" "$work/mnt/efi"; mount "$p2" "$work/mnt/a"; mount "$p3" "$work/mnt/b"; mount "$p4" "$work/mnt/state"
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
cat >"$work/grub.cfg" <<EOF
set timeout=5
set default=0
insmod all_video
insmod part_msdos
insmod fat
insmod ext2

menuentry 'RIGOS ${RIGOS_IMAGE_VERSION}' {
    search --no-floppy --label RIGOS_ROOT_A --set=root
    linux /live/vmlinuz boot=live components live-media=/dev/disk/by-label/RIGOS_ROOT_A live-media-path=/live ro noeject noautologin console=ttyS0,115200n8 console=tty0
    initrd /live/initrd.img
}
menuentry 'RIGOS ${RIGOS_IMAGE_VERSION} -- safe mode' {
    search --no-floppy --label RIGOS_ROOT_A --set=root
    linux /live/vmlinuz boot=live components live-media=/dev/disk/by-label/RIGOS_ROOT_A live-media-path=/live ro noeject noautologin nomodeset console=ttyS0,115200n8 console=tty0
    initrd /live/initrd.img
}
menuentry 'RIGOS ROOT_B fallback' {
    search --no-floppy --label RIGOS_ROOT_B --set=root
    linux /live/vmlinuz boot=live components live-media=/dev/disk/by-label/RIGOS_ROOT_B live-media-path=/live ro noeject noautologin console=ttyS0,115200n8 console=tty0
    initrd /live/initrd.img
}
EOF
install -m 0644 "$work/grub.cfg" "$work/mnt/a/boot/grub/grub.cfg"
install -m 0644 "$work/grub.cfg" "$work/mnt/b/boot/grub/grub.cfg"
sync
umount "$work/mnt/state" "$work/mnt/b" "$work/mnt/a" "$work/mnt/efi"
root_a_sha="$(sha256sum "$p2" | cut -d' ' -f1)"; root_b_sha="$(sha256sum "$p3" | cut -d' ' -f1)"
sync; losetup -d "$loop"; rm -f "${loop}p"{1,2,3,4}; trap - EXIT

output="$repo/dist/usb"
mkdir -p "$output"
image_name="rigos-usb-amd64-${RIGOS_IMAGE_VERSION}.img"
recovery_name="rigos-recovery-amd64-${RIGOS_IMAGE_VERSION}.iso"
manifest_name="rigos-usb-amd64-${RIGOS_IMAGE_VERSION}.build-manifest.json"
rm -f "$output/$image_name" "$output/$image_name.sha256" \
  "$output/$recovery_name" "$output/$recovery_name.sha256" \
  "$output/$manifest_name"
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
  '{schema:"rigos.image-build-manifest/v2",product:"RIGOS",product_version:$version,image_id:"rigos-usb-amd64",image_version:$version,image_channel:$channel,source_commit:$commit,source_date_epoch:$epoch,target:"x86_64-unknown-linux-gnu",base:"Debian GNU/Linux 12",kernel:$kernel,artifact:$artifact,artifact_sha256:$sha,artifact_size_bytes:$size,root_a_sha256:$root_a,root_b_sha256:$root_b,root_payload_sha256:$payload,layout:$layout[0],components:[$xmrig[0]],tools:{rustc:"1.85.1",live_build:"20230502",grub:"2.06"}}' \
  >"$output/$manifest_name"

"$source_root/scripts/verify-usb-appliance.sh" "$output/$image_name" "$output/$manifest_name"
"$source_root/scripts/verify-usb-image.sh" "$output/$recovery_name"
printf 'RIGOS appliance: %s\nRecovery ISO: %s\n' "$output/$image_name" "$output/$recovery_name"
