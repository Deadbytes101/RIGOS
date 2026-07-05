# RIGOS 0.0.4-alpha.4 USB Appliance

The authoritative persistent artifact is
`rigos-usb-amd64-0.0.4-alpha.4.img`.

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

`rigos-recovery-amd64-0.0.4-alpha.4.iso` is stateless recovery media. It is not
a persistent appliance and never runs the state grow helper.

Alpha four fixes the first-boot terminal stream contract found during physical
Dell 07GP33 testing. Whiptail keeps stdout attached to tty1 for screen rendering
and returns selected values through stderr, which the helper captures.

Build from a tracked-clean commit:

```bash
podman build -t rigos-usb-builder -f build/usb/Dockerfile .
podman run --rm --privileged \
  -v "$PWD:/source" -v /var/tmp/rigos-build:/work rigos-usb-builder
sha256sum -c dist/usb/rigos-usb-amd64-0.0.4-alpha.4.img.sha256
```

The builder exports `HEAD` with `git archive`. Untracked workspace files cannot
enter the image.

Historical evidence:

```text
0.0.4-alpha.1  GPT image reached QEMU but Dell firmware reported OS NOT FOUND
0.0.4-alpha.2  MBR image reached GRUB ROOT_A multi-user and password setup
0.0.4-alpha.3  booted normally but stdout piping hid every whiptail screen
0.0.4-alpha.4  terminal stream repair awaiting image and physical retest
```

Alpha four is not a release candidate. Physical boot, state, internal disk,
pool, power-loss and USB write gates still apply.
