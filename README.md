# RIGOS

Local-first CPU appliance delivered as a bootable USB image.

Current development preview is `RIGOS 0.0.4-alpha.6`.

The persistent appliance uses a raw MBR disk image for Legacy BIOS and removable-media UEFI boot.

```text
partition 1  EFI_SYSTEM FAT32 active
partition 2  RIGOS_ROOT_A
partition 3  RIGOS_ROOT_B
partition 4  RIGOS_STATE_SEED
```

The recovery ISO is stateless and does not grow the state partition.

## Alpha history

```text
0.0.4-alpha.1  GPT image failed Dell Legacy BIOS before GRUB
0.0.4-alpha.2  MBR image reached GRUB ROOT_A systemd and password setup
0.0.4-alpha.3  fixed console order but kept the first boot screen hidden
0.0.4-alpha.4  keeps the first boot screen on tty and captures answers separately
0.0.4-alpha.5  adds local rig profiles and portable XMRig Flight Sheets
0.0.4-alpha.6  adds visible machine-wide huge page authority
```

Alpha five is frozen at its physically validated image and Alpha six develops
performance authority on a separate branch.

## Verification

```bash
./scripts/verify.sh
```

## Local inspection

```bash
cargo run -p rigosd -- machine inspect
cargo run -p rigosd -- machine inspect --json
cargo run -p rigosd -- miner inspect --json
cargo run -p rigosd -- doctor --json
```

See [architecture](docs/architecture.md), [USB image build](docs/usb-image-build.md), [product contract](docs/product-contract.md), [pool contract](docs/pool-contract.md), [release claims](docs/release-claims.md), and [physical evidence policy](docs/physical-validation-evidence.md).
