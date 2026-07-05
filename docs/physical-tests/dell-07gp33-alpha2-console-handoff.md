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
- safe mode continued to about 70.9 seconds
- the last visible lines were HDA audio codec and input device registration
- no first boot dialog appeared after those lines

Current boot arguments place the serial console after the local console

```text
console=tty0 console=ttyS0,115200n8
```

The first boot service is bound to `/dev/tty1` and waits for network online before starting.

The safe mode entry also disables APIC and ACPI

```text
nomodeset noapic noacpi
```

The next physical diagnostic must use the normal first entry, not safe mode, and only reverse the console order while enabling status output

```text
console=ttyS0,115200n8 console=tty0 systemd.show_status=yes loglevel=7 debug=1
```

Do not add `noapic` or `noacpi` for that test.

This test does not prove ROOT_A mount state initialization miner start or internal disk safety.
