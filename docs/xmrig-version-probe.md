# XMRig Version Probe

Version precedence is API, trusted metadata, bounded executable probe, then unknown. Dynamic probing is optional and fails closed.

On Linux the validated ELF is launched directly with `--version`, an empty environment except deterministic locale, null stdin and bounded captured output. It receives a dedicated process group. Timeout sends SIGTERM to that verified group, escalates to SIGKILL after 250 ms, and reaps the direct child. The active miner PID is never accepted by the termination API.

