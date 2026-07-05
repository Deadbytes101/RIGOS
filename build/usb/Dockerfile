FROM rust:1.85.1-bookworm

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      ca-certificates curl dosfstools e2fsprogs fdisk gdisk git grub-efi-amd64-bin \
      grub-pc-bin grub2-common isolinux jq live-build mtools ovmf parted qemu-system-x86 \
      ripgrep rsync squashfs-tools syslinux-common syslinux-utils udev uuid-runtime xorriso zstd \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /source
ENTRYPOINT ["./scripts/build-usb-image.sh"]
