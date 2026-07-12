# RIGOS 0.0.4-alpha.25 USB Appliance

The Alpha.25 authority is:

```text
tag:    v0.0.4-alpha.25
source: ba02eb7429683550512b703cd4646d4d9ee6a888
```

The normal appliance image is a raw MBR USB image. It is separate from
the recovery ISO.


IMAGE LAYOUT
------------

```text
LBA0                    GRUB BIOS boot code
LBA1 through LBA2047    GRUB core embedding gap
partition 1             active FAT32 EFI_SYSTEM
partition 2             RIGOS_ROOT_A
partition 3             RIGOS_ROOT_B
partition 4             RIGOS_STATE_SEED
```

Legacy BIOS boots through MBR code in LBA0. UEFI boots through
`EFI/BOOT/BOOTX64.EFI` on partition 1. ROOT_A is the default. ROOT_B is
the manual fallback.


STATE GROW CONTRACT
-------------------

The final state partition already exists in the image. First boot may
only grow partition 4 after proving the exact boot USB, deterministic
MBR disk signature, partition starts, partition types, active flag,
filesystem labels and parent block-device topology.


RECOVERY ISO
------------

The documented Alpha.25 release assets are:

```text
rigos-recovery-amd64-0.0.4-alpha.25.iso
rigos-recovery-amd64-0.0.4-alpha.25.iso.sha256
```

The recovery ISO is stateless diagnostics and repair media. It is not
the persistent appliance image and does not have the same role as the
normal USB image.


BUILD NOTE
----------

The builder exports source from a tracked-clean commit with `git
archive`. Untracked workspace files cannot enter the image.

Example local build:

```bash
podman build -t rigos-usb-builder -f build/usb/Dockerfile .
podman run --rm --privileged \
  -v "$PWD:/source" -v /var/tmp/rigos-build:/work rigos-usb-builder
```

Verify SHA256 before use.


STATUS
------

Alpha.25 is a functional physical Alpha preview. It is not a release
candidate, stable release or production-ready release.
