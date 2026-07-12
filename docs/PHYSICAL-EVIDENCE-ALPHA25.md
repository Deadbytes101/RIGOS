PHYSICAL EVIDENCE ALPHA.25
==========================

This page records sanitized physical evidence only. It excludes wallet
identity, API token contents, password material, SSH private material,
complete private runtime config and persistent-state dumps.


VERIFIED NODE BASELINE
----------------------

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


LATEST OPERATOR STATUS SNAPSHOTS
--------------------------------

These snapshots were recorded from the short operator command on real
hardware after firstboot, pool application and configured mining.

```text
rigosadmin@rig01:~$ rig status
version: 0.0.4-alpha.25
node: rig01
state: ready
configuration: unavailable
revision: ba31f51f-0983-488a-aa6b-c110dddfe6c6
network: ready
miner: active
algorithm: rx/0
pool: 139.99.69.109:10001
hashrate: 797.66 H/s
shares: accepted=40 rejected=0
huge_pages: 1172/1172
health: ready
last_recovery_action: none
```

```text
rigosadmin@rig02:~$ rig status
version: 0.0.4-alpha.25
node: rig02
state: ready
configuration: unavailable
revision: 042c11c7-c3e8-458b-ac43-d3920557b7bb
network: ready
miner: active
algorithm: rx/0
pool: 139.99.69.109:10001
hashrate: 337.4 H/s
shares: accepted=63 rejected=0
huge_pages: 1170/1170
health: ready
last_recovery_action: none
```


EARLIER RECORDED RIG02 SAMPLE
-----------------------------

```text
hashrate_10s approximately 338 H/s
hashrate_60s approximately 340 H/s
highest approximately 341.81 H/s
accepted_shares=14
rejected_shares=0
```

These figures are recorded physical samples. They are not guaranteed
benchmarks for other hardware.


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
