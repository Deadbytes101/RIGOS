# Physical Validation Evidence

Public Git evidence is sanitized and regression-testable. Complete raw evidence is retained outside Git as an encrypted `tar.zst.age` archive. Both use one run ID, source commit and authoritative binary SHA-256.

## Collection

The collector is deliberately phased because v0.0.1 must not control XMRig lifecycle. The operator establishes each miner state, then invokes the authoritative RC's `validation-tools/collect-physical-validation.sh` with the same run ID and output directory. Baseline/finalize phases accept explicit XMRig binary/config paths for zero-mutation hashes. The probe phase uses the probe-helper ELF packaged in that same RC, never production XMRig and never a locally rebuilt helper.

Raw output must remain outside the repository. Before packaging, the operator reviews `raw-meta/result-input.json` and changes a check from `blocked` only when its raw evidence proves `pass`, `fail`, or `not_applicable`.

```bash
./scripts/collect-physical-validation.sh --run-id "$RUN_ID" --binary ./rigosd --output "$RAW" --phase baseline --xmrig /path/to/xmrig --config /path/to/config.json
./scripts/collect-physical-validation.sh --run-id "$RUN_ID" --binary ./rigosd --output "$RAW" --phase miner-stopped
./scripts/collect-physical-validation.sh --run-id "$RUN_ID" --binary ./rigosd --output "$RAW" --phase miner-running-no-api
./scripts/collect-physical-validation.sh --run-id "$RUN_ID" --binary ./rigosd --output "$RAW" --phase miner-running-loopback-api
./scripts/collect-physical-validation.sh --run-id "$RUN_ID" --binary ./rigosd --output "$RAW" --phase probe-timeout --probe-helper ./probe_helper
./scripts/collect-physical-validation.sh --run-id "$RUN_ID" --binary ./rigosd --output "$RAW" --phase finalize --xmrig /path/to/xmrig --config /path/to/config.json
```

## Encryption

- Backend: native age X25519 recipients only.
- Packaging receives public recipients only through an ignored recipient file.
- Private identities remain outside the repository, VM, rigs and CI.
- Compression precedes encryption: tar, zstd, age.
- Packaging writes `.partial`, verifies non-zero output, hashes it, then atomically renames it.
- A run remains blocked until `verify-private-archive.ps1` decrypts and validates inner checksums with an operator identity.

Create identities outside the project with `age-keygen -o <identity>` and derive recipients with `age-keygen -y <identity>`. Maintain a separate offline recovery identity where possible.

Packaging requires PowerShell 7.4, `age`, `zstd`, `tar`, Rust/Cargo, the external raw directory and an ignored recipient file. Private verification additionally requires the operator identity and updates the public manifest from `blocked` to `pass` only when the result checks already pass.

```text
RAW DATA MUST NEVER ENTER GIT HISTORY.
SANITIZED DATA MUST STILL PROVE THE RESULT.
THE PRIVATE ARCHIVE HASH COMMITS TO THE RAW EVIDENCE.
```
