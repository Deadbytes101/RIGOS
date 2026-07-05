# RIGOS 0.0.4-alpha.1 USB Appliance

The authoritative persistent artifact is
`rigos-usb-amd64-0.0.4-alpha.1.img`, a raw GPT image for Rufus/DD. The image
contains BIOS boot, EFI, identical read-only ROOT_A/ROOT_B slots, and a valid
ext4 `RIGOS_STATE_SEED` final partition. First boot may only grow the existing
state partition after proving the exact boot USB and immutable layout.

`rigos-recovery-amd64-0.0.4-alpha.1.iso` is explicitly stateless. It is not a
persistent appliance and never edits a partition table.

Build from a tracked-clean commit:

```bash
podman build -t rigos-usb-builder -f build/usb/Dockerfile .
podman run --rm --privileged \
  -v "$PWD:/source" -v /var/tmp/rigos-build:/work rigos-usb-builder
sha256sum -c dist/usb/rigos-usb-amd64-0.0.4-alpha.1.img.sha256
```

The builder exports `HEAD` with `git archive`; untracked workspace files cannot
enter the image. Flashing overwrites the selected USB device. This alpha is not
a release candidate and still requires the documented physical gates.
