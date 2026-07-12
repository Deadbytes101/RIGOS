PHYSICAL EVIDENCE ALPHA.25
==========================

This page records sanitized physical evidence only. It excludes wallet
identity, API token contents, password material, SSH private material,
complete private runtime config and persistent-state dumps.


VERIFIED NODE
-------------

```text
node_name=rig02
version=0.0.4-alpha.25
build_commit=ba02eb7429683550512b703cd4646d4d9ee6a888
state=ready
persistent_device=/dev/sdb4
runtime=ready
network=ready
miner=active
algorithm=rx/0
exact_threads=2
pool_connected=true
huge_pages=100%
health=ready
restart_count=0
```


RECORDED PHYSICAL SAMPLE
------------------------

```text
hashrate_10s approximately 338 H/s
hashrate_60s approximately 340 H/s
highest approximately 341.81 H/s
accepted_shares=14
rejected_shares=0
```

These figures are one recorded physical sample. They are not a
guaranteed benchmark for other hardware.


DOCTOR CHECKS OBSERVED PASSING
------------------------------

```text
bounded volatile logs
machine inspection
miner health
miner inspection
mutation boundary
network readiness
huge-page readiness
runtime activation
persistent state readiness
```


LIMITS OF THIS EVIDENCE
-----------------------

This evidence does not prove broad hardware compatibility, repeated
power-cycle reliability, network outage recovery, pool outage recovery
or long unattended soak behavior.
