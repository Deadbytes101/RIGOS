# DBYTE RIGOS

Local-first, CPU-only mining machine inspection for Debian Linux.

```text
READ THE MACHINE.
UNDERSTAND THE MINER.
ESTABLISH THE CONTRACT.
MUTATE NOTHING.
```

`v0.0.1` is an observation contract. It reads Linux machine state and an existing local XMRig process; it does not start, stop, signal, configure, supervise, download, or update a miner.

## Commands

```bash
cargo run -p rigosd -- machine inspect
cargo run -p rigosd -- machine inspect --json
cargo run -p rigosd -- miner inspect --json
cargo run -p rigosd -- doctor --json
./scripts/verify.sh
```

Optional local fallbacks:

```bash
rigosd --xmrig-executable /usr/local/bin/xmrig --xmrig-config /etc/xmrig/config.json miner inspect --json
```

The API endpoint is never accepted from the CLI. API inspection is derived only from the active XMRig configuration and connects only to validated loopback addresses.

## Platform contract

- Canonical future OS base: Debian 13 amd64
- Binary ABI floor: Debian 12 amd64
- Tested runtimes: Debian 12 and Debian 13
- Release CPU target: generic x86-64; never `target-cpu=native`
- Cloud control, accounts, subscriptions, LAN fleet control and ISO construction are out of scope

See [architecture](docs/architecture.md), [JSON contract](docs/json-cli-contract.md), and [threat model](docs/threat-model.md).

