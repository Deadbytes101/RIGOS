RIGOS OPERATIONS
================

RIGOS Alpha.25 has two local command surfaces:

```text
rig       short operator command
rigosctl  detailed authority and inspection command
```


DAILY COMMANDS
--------------

```bash
rig status
rig health
rig logs --miner
rig logs --since 10m
rig config
rig recover
```

Mutating commands require explicit root intent:

```bash
sudo rig restart
sudo rig stop
sudo rig start
```

`rig` never invokes sudo internally.


STATUS INTERPRETATION
---------------------

`rig status` summarizes version, node name, state, configuration
revision, network state, miner state, algorithm, redacted pool endpoint,
hashrate, shares, huge pages, health and last recovery action.

`rig status --json` uses an explicit public allowlist. It does not print
wallet identity, API token paths, private hashes, raw runtime config,
passwords or SSH private material.


HEALTH EXIT STATES
------------------

```text
0  healthy or ready
1  degraded, warming up, waiting external or recovering
2  failed or blocked
3  not configured
4  unavailable or inspection failure
```


LOGS
----

Common filters:

```bash
rig logs --miner
rig logs --boot
rig logs --network
rig logs --health
rig logs --since 10m
rig logs --follow
```

Output is bounded by default. Streaming requires `--follow`.


CONTROLLED MINER RESTART
------------------------

`sudo rig restart` delegates to `rigos-miner.service` and then verifies
the postcondition. It does not bypass state readiness, runtime config,
profile or miner gates.


FIRSTBOOT BEHAVIOR
------------------

Unconfigured normal boot displays firstboot on tty1. The miner remains
stopped until configuration is committed and activation succeeds.

Manual firstboot execution is intentionally restricted:

```bash
sudo rig firstboot run
```

It must be local and must not race existing configuration or utility
mode.


RECOVERY BEHAVIOR
-----------------

`rig recover` is read-only by default.

Rollback, when supported, must be explicit:

```bash
sudo rig recover rollback
```

Alpha.25 does not pretend rollback happened when the operation is not
available.


COMMON FAILURE STATES
---------------------

```text
not configured
  firstboot is required before mining

waiting external
  network or pool condition is the likely blocker

degraded
  miner or observer evidence is incomplete or below threshold

blocked
  runtime config, revision or gate truth prevents safe mining
```
