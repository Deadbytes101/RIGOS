# Local rig configuration

RIGOS accepts a local data file named `rigos.conf` from the FAT32 `EFI_SYSTEM` partition of the exact verified boot USB.

The file exists to make repeated appliance setup fast without adding a cloud account or remote control plane.

## Contract

- local data only
- never executed as shell
- exact verified boot USB only
- unknown keys rejected
- duplicate keys rejected
- malformed quoting rejected
- command substitution rejected
- administrator passwords rejected
- cloud account credentials rejected
- remote access endpoints rejected
- canonical policy written atomically
- normalized copy stored with restrictive permissions

## Version one fields

```text
RIGOS_CONFIG_VERSION=1
NODE_NAME=rig01
TIMEZONE=Asia/Bangkok
POOL_HOST=gulf.moneroocean.stream
POOL_PORT=10128
POOL_TLS=required
MINING_IDENTITY=
CPU_THREADS=0
HUGE_PAGES=1
WATCHDOG_ENABLED=0
AUTO_START=1
```

## Import flow

1. The state verifier proves the boot parent and MBR layout.
2. RIGOS reads `rigos.conf` from partition one of that exact disk.
3. The file is copied into tmpfs before parsing.
4. Valid values prefill the visible first boot UI.
5. The operator confirms the values locally.
6. RIGOS writes `policy.json` and `xmrig.json` atomically.
7. A normalized local copy is stored under `/var/lib/rigos`.

A missing file falls back to the normal interactive setup.

The format is RIGOS owned. External appliance files are reference input only and are not copied into the implementation.
