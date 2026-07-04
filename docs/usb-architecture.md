# USB-Native Architecture

The future RIGOS appliance runs from a flashed USB device. Internal HDDs and SSDs are not installation or runtime-state targets.

```text
RIGOS USB
├── BIOS_BOOT       1–2 MiB
├── EFI_SYSTEM      256 MiB FAT32
├── RIGOS_ROOT_A    signed read-only system image
├── RIGOS_ROOT_B    signed read-only fallback image
└── RIGOS_STATE     persistent ext4 state
```

Baseline: 16 GiB recommended capacity, USB 2.0 minimum interface, x86-64, Legacy BIOS and UEFI.

## Write policy

- `/tmp`, `/run`, and `/var/tmp` use tmpfs.
- Journaling and event storage are volatile or strictly bounded.
- zram replaces disk/USB-backed swap.
- General root is read-only; persistent writes are restricted to explicit state paths.
- Policy/configuration revisions use atomic replacement, checksums, and last-known-good fallback.
- XMRig logs and telemetry must never grow without a fixed byte/count/retention bound.

## Internal disks

RIGOS may discover read-only metadata. Auto-mount, formatting, installation, swap, and mining-data writes on internal disks are forbidden by default. Removing the USB removes RIGOS identity, policy, and runtime from the machine.

Image creation is the v0.0.4 milestone. v0.0.1 establishes observation and namespace contracts without claiming a bootable appliance already exists.
