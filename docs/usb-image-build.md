# RIGOS 0.0.4-alpha.3 USB Appliance

The authoritative persistent artifact is
`rigos-usb-amd64-0.0.4-alpha.3.img`.

It is a raw MBR image for Rufus DD mode or `dd`.

The image layout is fixed:

```text
LBA0                    GRUB BIOS boot code
LBA1 through LBA2047    GRUB core embedding gap
partition 1             active FAT32 EFI_SYSTEM
partition 2             RIGOS_ROOT_A
partition 3             RIGOS_ROOT_B
partition 4             RIGOS_STATE_SEED
```

Legacy BIOS boots through MBR code in LBA0. UEFI boots through
`EFI/BOOT/BOOTX64.EFI` on partition 1. ROOT_A is the default. ROOT_B is the
manual fallback.

The final state partition already exists in the image. First boot may only
grow partition 4 after proving the exact boot USB, deterministic MBR disk
signature, partition starts, partition types, active flag, filesystem labels
and parent block-device topology.

`rigos-recovery-amd64-0.0.4-alpha.3.iso` is stateless recovery media. It is not
a persistent appliance and never runs the state grow helper.

Alpha three fixes the first-boot terminal contract found during physical Dell
07GP33 testing. Whiptail renders directly on tty1 and returns selected values
through output file descriptor one.

Build from a tracked-clean commit:

```bash
podman build -t rigos-usb-builder -f build/usb/Dockerfile .
podman run --rm --privileged \
  -v "$PWD:/source" -v /var/tmp/rigos-build:/work rigos-usb-builder
sha256sum -c dist/usb/rigos-usb-amd64-0.0.4-alpha.3.img.sha256
```

The builder exports `HEAD` with `git archive`. Untracked workspace files cannot
enter the image.

Historical evidence:

```text
0.0.4-alpha.1  GPT image reached QEMU but Dell firmware reported OS NOT FOUND
0.0.4-alpha.2  MBR image reached GRUB ROOT_A multi-user and password setup
0.0.4-alpha.3  first-boot terminal repair awaiting image and physical retest
```

Alpha three is not a release candidate. Physical boot, state, internal disk,
pool, power-loss and USB write gates still apply.
