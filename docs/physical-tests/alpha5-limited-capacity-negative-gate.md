# Alpha five limited capacity negative gate

This is a required negative physical test, not evidence that persistent state or the positive configuration flow passed.

## Preconditions

- boot the Alpha.5 candidate from the exact test USB
- preserve a `limited_capacity` state outcome
- place a valid synthetic `rig.conf` and Flight Sheet on EFI_SYSTEM
- do not place credentials or corpus identifiers in the fixture

## Expected result

- firstboot displays the state outcome and blocks configuration
- `rigosadmin` has a locally established password so the operator can collect Issue 3 diagnostics
- no config source is mounted or parsed
- no revision, policy, XMRig config, identity or mapping is written
- timezone is unchanged
- miner enabled and running states are unchanged
- XMRig does not start

Capture the redacted state status, unit state, timezone and hashes or absence of state files. Positive config-flow testing remains blocked by the separate state issue until the same machine reports `ready` or `grown` across reboot.
