# RIGOS USB Image (Bootstrap)

This bootstrap image is a Debian 12 `iso-hybrid` image for Rufus/DD. It uses
ISOLINUX/SYSLINUX for Legacy BIOS and GRUB for UEFI. Its SquashFS root is
read-only.

The first boot opens a console TUI, accepts an arbitrary compatible
`host:port`, validates the mining identity and writes `policy.json` and
`xmrig.json` atomically. XMRig is pinned to upstream version 6.26.0 and its
official SHA-256 is verified during the image build.

On first boot, the state initializer identifies the live boot device and only
creates an ext4 `RIGOS_STATE` partition when udev confirms that device uses the
USB bus, its capacity is at least 8 GB, and partition 3 does not already exist.
It refuses internal, ambiguous, or undersized devices. The bootstrap does not
yet claim the final signed ROOT_A/ROOT_B updater; that remains the next
hardening milestone.

Build from a clean tree in the privileged Debian image builder:

```bash
podman build -t rigos-usb-builder -f build/usb/Dockerfile .
podman run --rm --privileged -v "$PWD:/source" -v rigos-live-work:/work rigos-usb-builder
sha256sum -c dist/usb/rigos-usb-amd64.iso.sha256
```

Flash `dist/usb/rigos-usb-amd64.iso` with Rufus in DD mode. This overwrites the
selected USB device; verify the target device before writing.
