# v0.0.1 Architecture

```mermaid
flowchart LR
  CLI[rigosd CLI] --> Machine[rigos-machine]
  CLI --> Miner[MinerBackend]
  Miner --> Xmrig[XmrigBackend]
  Xmrig --> Proc[read-only /proc]
  Xmrig --> Config[read-only JSON config]
  Xmrig --> API[restricted loopback API]
  Xmrig --> Probe[isolated version probe]
  Machine --> Model[typed snapshots]
  Xmrig --> Model
  Model --> Envelope[CLI envelope v1]
  Envelope --> Human[human renderer]
  Envelope --> JSON[JSON renderer]
```

Machine discovery is independent from miner-specific parsing. `InspectedProcessIdentity` provides identity only; lifecycle authority does not exist. `ProbeJobHandle` is the sole process-termination capability and can terminate only its own isolated probe group.

## Version boundary

- v0.0.1: observation and diagnostics
- v0.0.2: explicit local authority, lifecycle ownership, systemd integration, crash recovery and thermal FSM

