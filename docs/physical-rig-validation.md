# Physical Rig Validation

Run on at least one Athlon II or Phenom II class Debian machine as an unprivileged user:

1. Run `./scripts/verify.sh` and preserve output.
2. Capture `rigosd machine inspect --json` with real hwmon and huge-page data.
3. Capture `rigosd miner inspect --json` with XMRig stopped, running without API, and running with loopback API.
4. Confirm no illegal-instruction fault and no process/configuration changes.
5. Record Debian version, kernel, CPU model, XMRig version and artifact SHA-256.

Containers and VMs do not satisfy this physical acceptance tier.

Use the phased collector described in [physical-validation-evidence.md](physical-validation-evidence.md). Validation is not accepted until the tested binary matches the authoritative RC checksum, public evidence passes secret scanning, and the external age archive passes independent decryptability verification.

The authoritative RC directory is self-contained for collection: use
`validation-tools/collect-physical-validation.sh` and
`validation-tools/probe_helper` from that directory. Do not rebuild the helper
or copy a collector from another commit; verify the complete RC directory with
`sha256sum -c SHA256SUMS` before collection.
