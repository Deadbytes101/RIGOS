# Dell 07GP33 alpha two physical result

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

Confirmed

- firmware accepted the MBR USB
- GRUB menu displayed
- default ROOT_A entry launched the kernel
- USB mass storage was detected as a removable SCSI disk
- kernel initialization completed without a visible panic
- zram swap was created
- systemd reached `multi-user.target`
- systemd reached `graphical.target`
- `rigos-firstboot.service` started
- the local administrator password step completed

The normal entry only showed the local userspace console after reversing console order

```text
console=ttyS0,115200n8 console=tty0
```

The original safe mode entry used

```text
nomodeset noapic noacpi
```

That path was not a valid baseline on this board. Alpha three keeps `nomodeset` but removes `noapic` and `noacpi`.

First boot failure

The next whiptail dialog after password completion was invisible. The firstboot helper captured both stdout and stderr while whiptail renders its UI on stderr. The service remained blocked waiting for input on an unseen dialog.

Alpha three repair

```text
whiptail --output-fd 1
stdout captured for selected values
stderr inherited by tty1 for UI rendering
```

Accepted from alpha two

```text
physical MBR firmware boot
GRUB
ROOT_A kernel launch
ROOT_A userspace
systemd multi-user
firstboot password step
```

Not accepted from alpha two

```text
visible complete firstboot flow
policy persistence
miner start
state growth result
internal disk unchanged proof
```
