# Dell 07GP33 alpha two console handoff

Board

```text
Dell 07GP33
BIOS A12
```

Artifact

```text
rigos-usb-amd64-0.0.4-alpha.2.img
4028775aafdae00cb6b9a124ecd96db8ecc9dea16a29d655e920f01e6c14ead4
```

Observed

- firmware accepted the MBR USB
- GRUB menu displayed
- default entry launched the kernel
- safe mode launched the kernel
- USB mass storage was detected as a removable SCSI disk
- no kernel panic was visible in the captured frames
- local display cleared to a black screen with a cursor before the first boot dialog appeared

Current boot arguments place the serial console after the local console

```text
console=tty0 console=ttyS0,115200n8
```

The first boot service is bound to `/dev/tty1` and waits for network online before starting. The next physical diagnostic must place the local console last and show systemd status

```text
console=ttyS0,115200n8 console=tty0 systemd.show_status=yes loglevel=7
```

This test does not prove ROOT_A mount state initialization miner start or internal disk safety.
