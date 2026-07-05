# RIGOS

CPU-only USB appliance for local Linux mining rigs.

Current preview is `RIGOS 0.0.4-alpha.3`.

The persistent appliance is a raw MBR disk image. It supports Legacy BIOS through GRUB boot code in LBA0 and removable-media UEFI through `EFI/BOOT/BOOTX64.EFI`.

```text
partition 1  EFI_SYSTEM FAT32 active
partition 2  RIGOS_ROOT_A
partition 3  RIGOS_ROOT_B
partition 4  RIGOS_STATE_SEED
```

The recovery ISO is stateless and does not grow the state partition.

## Alpha history

```text
0.0.4-alpha.1  GPT image passed QEMU and failed Dell Legacy BIOS before GRUB
0.0.4-alpha.2  MBR image reached GRUB ROOT_A systemd and password setup
0.0.4-alpha.3  repairs first boot terminal rendering and console order
```

Alpha three still requires a new image build, checksum, QEMU boot matrix and physical first-boot completion.

## Build checks

```bash
./scripts/verify.sh
```

## Local inspection commands

```bash
cargo run -p rigosd -- machine inspect
cargo run -p rigosd -- machine inspect --json
cargo run -p rigosd -- miner inspect --json
cargo run -p rigosd -- doctor --json
```

## Product boundaries

- local-first operation
- pool-neutral configuration
- generic x86-64 release target
- no automatic internal disk installation
- exact USB parent verification before state growth
- official pinned XMRig binary with recorded provenance

See [architecture](docs/architecture.md), [USB image build](docs/usb-image-build.md), [product contract](docs/product-contract.md), [pool contract](docs/pool-contract.md), [release claims](docs/release-claims.md), and [physical evidence policy](docs/physical-validation-evidence.md).
