# Threat Model

## Protected assets

- Existing miner uptime and configuration
- API access tokens and pool credentials
- Host integrity and process ownership
- Trustworthy, non-fabricated observations

## Controls

- No shell execution, PATH search, miner mutation or persistent writes
- `/proc` races and permission failures degrade to diagnostics
- API targets originate only from active config and must validate as loopback
- No redirects, proxies, arbitrary paths, URL input or port scanning
- Bounded HTTP timeouts and response sizes
- Executable probe rejects scripts, setuid/setgid and non-ELF files
- Probe runs in a dedicated process group with bounded output and cleanup
- Active miner identity is structurally separate from probe termination capability

## Residual risks

- `/proc` and executable replacement races cannot be fully eliminated without stronger kernel handles; unsafe probes fail closed
- A wildcard XMRig bind may expose the API to the LAN; inspection reports this but does not mutate configuration
- PID namespace and network namespace traversal are unsupported
- v0.0.1 does not enforce thermal safety; it only observes sensors

