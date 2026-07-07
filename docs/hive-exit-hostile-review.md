# Hive Exit hostile review

The first stability batch passed source verification, but review found two release blockers before image construction:

1. An explicit inspector configuration must take precedence over a configuration discovered from the running process command line.
2. The public XMRig view must be constructed from an allowlist. Copying the private configuration and deleting known secret fields is not an acceptable security boundary.

The review fix must pass Rust formatting, the `rigos-xmrig` unit suite, the runtime redaction fixture, and `git diff --check` before it is committed to the stability branch.
