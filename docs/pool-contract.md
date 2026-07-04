# Pool-Neutral Contract

RIGOS accepts arbitrary compatible Stratum endpoints. Built-in templates are convenience metadata and never a whitelist.

Profiles normalize into `PoolProfile`, then pass endpoint, TLS, authentication, failover, backend, and algorithm compatibility validation before a miner backend may compile configuration.

```text
POOL TEMPLATE
      ↓
NORMALIZED POOL PROFILE
      ↓
COMPATIBILITY VALIDATION
      ↓
MINER BACKEND COMPILER
      ↓
GENERATED XMRIG CONFIG
      ↓
XMRIG
```

Template identities: MoneroOcean, 2Miners, NiceHash Stratum, SupportXMR, HeroMiners, HashVault, Nanopool, and Custom Stratum. Endpoint details remain operator-controlled and are not hard-coded as a forced list.

Universal protocol support is not claimed. Unsupported algorithms or backend combinations return explicit compatibility errors. Lifecycle/config generation remains scheduled for v0.0.3; v0.0.1 includes the normalized contract and validation boundary only.
