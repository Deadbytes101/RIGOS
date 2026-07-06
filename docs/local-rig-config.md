# Local Rig Profile and Flight Sheets

RIGOS reads portable configuration only from partition one of the exact USB proven by the state verifier. The attestation records the boot ID, disk and partition major/minor identities, MBR PTUUID, partition PARTUUID, root identity and verification outcome. The config engine reruns the verifier and compares those stable identities before every read-only mount; `/dev/sdX` paths are advisory only.

It stages bounded regular files in tmpfs before parsing and never sources configuration as shell.

## USB layout

```text
/rigos/rig.conf
/rigos/flight-sheets/*.json
/rigos/import/*.json
```

`rig.conf` controls machine identity, timezone, deterministic Flight Sheet selection and miner start policy. See `configs/rigos.conf.example`.

`FLIGHT_SOURCE` is `native`, `import` or `interactive`. Native and import modes require one safe `FLIGHT_REF`; interactive mode forbids it and never auto-selects a file. `MINER_START_MODE` is `manual` or `on_boot`. Legacy `AUTO_START`, watchdog and split active/import selection keys are rejected.

Flight Sheets describe only an XMRig workload: algorithm, ordered pools, TLS, a local identity reference, worker template and CPU policy. They cannot start services or contain a wallet, pool username, password or cloud credential. See `configs/flight-sheets/xmr-ssl.json`.

## Local identities

The operator resolves `identity_ref` on the local TTY. Values are stored only in the verified persistent state under `/var/lib/rigos/identities`, protected from normal diagnostics, and copied only into the restricted runtime `xmrig.json`. Canonical policy stores the alias, not the value.

External `wal_id` values remain external references. They are never sent to XMRig. Confirmed mappings are local state and are not copied back to EFI_SYSTEM.

## Offline Hive-style import

The importer is a one-way compatibility boundary, not a Hive runtime. It accepts XMRig workload fields, ordered pool URLs, TLS, algorithm, placeholders and whitelisted CPU JSON fragments. `%URL%`, `%WAL%` and `%WORKER_NAME%` become RIGOS proposal inputs resolved locally before confirmation.

Hive API endpoints, rig and farm IDs, rig passwords, remote access fields, tokens, unsupported miners and dangerous unknown fields are rejected. Lifecycle fields are reported but cannot alter machine start policy. Import provenance contains only a source filename, SHA-256, timestamp and redacted warnings.

## Failure and recovery

Parsing and validation finish before persistent or service mutation. Invalid input displays a stable code, safe location and redacted explanation. The operator may retry, inspect diagnostics, explicitly discard the import for the current boot and configure manually, or reboot. RIGOS never edits the source file.

Configuration commits capture the current timezone and miner enabled/running state, stop XMRig, create a complete revision and atomically switch the current pointer. Timezone, unit policy and miner start are then applied in order. Failure restores the previous pointer and runtime snapshot; if restoration is incomplete XMRig remains stopped and the pending transaction is retained for boot recovery. `rigos.nomine=1` blocks mining for one boot without changing persistent policy.

Only state outcome `ready` permits import. The status records `action=grown` or
`action=unchanged` separately. `limited_capacity` and every blocked outcome are
negative gates with zero config, timezone or miner mutation.
