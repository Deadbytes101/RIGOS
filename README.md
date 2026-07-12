RIGOS
=====

A Debian-based x86_64 USB compute appliance for automatic,
persistent and observable CPU mining.

CURRENT RELEASE
---------------

Version: 0.0.4-alpha.25
Status: Functional physical Alpha preview
Tag: v0.0.4-alpha.25
Source: ba02eb7429683550512b703cd4646d4d9ee6a888

Physically booted and verified on real hardware.
Not stable.
Not production-ready.


WHAT RIGOS IS
-------------

RIGOS is a local-first Linux USB appliance for CPU mining experiments.
It boots on x86_64 hardware, keeps the appliance root immutable, stores
operator state separately, and makes the local machine the authority.

RIGOS Alpha.25 provides:

```text
Debian 12 base
x86_64 USB appliance image
BIOS and UEFI boot paths where verified
persistent local state
atomic configuration revisions
physical firstboot for unconfigured nodes
automatic configured boot
hostname and machine profile application
persistent SSH host identity
RandomX/XMRig runtime generation
exact CPU thread control
huge-page preparation
network readiness inspection
systemd-owned miner lifecycle
restricted loopback XMRig observer
bounded miner health supervision
short operator command: rig
detailed authority command: rigosctl
stateless recovery ISO
```


WHAT RIGOS IS NOT
-----------------

RIGOS is not a Hive OS clone, cloud account system, hosted dashboard,
subscription service, remote shell product, billing platform, worker
limit system, forced-pool miner or production-ready distribution.

RIGOS does not remove the official XMRig upstream donation behavior.
It does not claim stable unattended operation until power-cycle,
network-failure, pool-failure and soak evidence exists.


BOOT AND AUTOMATION FLOW
------------------------

Configured boot:

```text
boot
  -> persistent state ready
  -> SSH identity ready
  -> committed configuration detected
  -> profile applied
  -> runtime configuration activated
  -> huge pages prepared
  -> network ready
  -> miner started
  -> pool work received
  -> hashrate observed
  -> health ready
```

Unconfigured boot:

```text
boot
  -> persistent state ready
  -> no committed configuration
  -> firstboot appears on tty1
  -> miner remains stopped
  -> configuration committed
  -> normal activation continues
```

Utility and recovery modes are separate from normal mining mode.


OPERATOR COMMANDS
-----------------

Alpha.25 adds the short local operator command:

```bash
rig status
rig health
rig start
rig stop
rig restart
rig logs
rig config
rig firstboot
rig recover
rig version
rig help
```

Aliases:

```bash
rig s
rig h
rig up
rig down
rig r
rig log
```

Examples:

```bash
rig status
rig health
rig logs --miner
rig logs --since 10m
sudo rig restart
rig config
rig recover
```

The `rig` command delegates to systemd, journalctl and rigosctl. It
does not hide sudo. Mutating commands require explicit root intent.
Start and restart do not bypass state, config or runtime gates.
Operator JSON uses explicit public allowlists and does not print raw
private runtime configuration.


SYSTEM ARCHITECTURE
-------------------

```text
USB image
  partition 1  EFI_SYSTEM
  partition 2  RIGOS_ROOT_A
  partition 3  RIGOS_ROOT_B
  partition 4  RIGOS_STATE_SEED

immutable root
  systemd units
  RIGOS authorities
  official XMRig binary

persistent state
  configuration revisions
  active current pointer
  SSH host identity
  recovery credential record
  miner health budget

runtime
  /run/rigos status files
  private xmrig.json
  public redacted views
  loopback XMRig API token
```

More detail: [Architecture](docs/architecture.md).


PERSISTENT STATE
----------------

Persistent state is stored separately from the immutable appliance
root. Configuration is committed as a revision and then activated.
The current revision pointer is switched atomically. Runtime files are
generated from the committed state and validated before the miner starts.


HEALTH AND SELF-RECOVERY
------------------------

RIGOS observes the miner through systemd, public runtime status, network
truth, bounded journal evidence and the restricted XMRig loopback API.
Health states include ready, warming up, waiting on external network or
pool conditions, degraded, blocked and unknown.

The supervisor uses bounded restart policy. It avoids uncontrolled
restart loops and preserves diagnostic access.


SECURITY BOUNDARIES
-------------------

RIGOS keeps private material out of public operator output:

```text
no wallet identity in README evidence
no password material
no API token contents
no SSH private keys
no private runtime config dumps
no persistent-state image dumps
no hidden sudo in rig
explicit recovery mutation
release hash verification required
```

More detail: [Security Model](docs/SECURITY-MODEL.md).


PHYSICAL ALPHA.25 EVIDENCE
--------------------------

Latest recorded operator snapshots:

```text
node=rig01
version=0.0.4-alpha.25
state=ready
configuration=unavailable
revision=ba31f51f-0983-488a-aa6b-c110dddfe6c6
network=ready
miner=active
algorithm=rx/0
pool=139.99.69.109:10001
hashrate=797.66 H/s
shares=accepted=40 rejected=0
huge_pages=1172/1172
health=ready
last_recovery_action=none
```

```text
node=rig02
version=0.0.4-alpha.25
state=ready
configuration=unavailable
revision=042c11c7-c3e8-458b-ac43-d3920557b7bb
network=ready
miner=active
algorithm=rx/0
pool=139.99.69.109:10001
hashrate=337.4 H/s
shares=accepted=63 rejected=0
huge_pages=1170/1170
health=ready
last_recovery_action=none
```

Earlier recorded rig02 sample:

```text
hashrate_10s approximately 338 H/s
hashrate_60s approximately 340 H/s
highest approximately 341.81 H/s
accepted_shares=14
rejected_shares=0
```

These are physical samples, not benchmark guarantees for other
hardware. Details: [Physical Alpha.25 Evidence](docs/PHYSICAL-EVIDENCE-ALPHA25.md).


DOWNLOAD AND VERIFICATION
-------------------------

Current release page:

```text
https://github.com/Deadbytes101/RIGOS/releases/tag/v0.0.4-alpha.25
```

Documented release assets:

```text
rigos-recovery-amd64-0.0.4-alpha.25.iso
rigos-recovery-amd64-0.0.4-alpha.25.iso.sha256
```

Verify SHA256 before use. The recovery ISO is diagnostics and repair
media; it is not the same role as the normal USB appliance image.
GitHub source zip and tar.gz files are source snapshots, not bootable
images.


PROJECT HISTORY
---------------

Alpha.22
  Repaired the physical firstboot/systemd transaction defect and
  published the first functional physical preview.

Alpha.23 and Alpha.24
  Hardened miner health, state identity and hostname/runtime behavior
  after physical defects were found.

Alpha.25
  Added the short rig operator interface, explicit operator JSON
  allowlists, automatic configured operation and physical health
  verification.

Current status
  Alpha.25 is the frozen functional milestone.
  Development is paused.
  The project is usable as an experimental appliance but is not declared
  stable or production-ready.

Detailed history: [Project History](docs/PROJECT-HISTORY.md).


CURRENT LIMITS
--------------

Still incomplete:

```text
broad hardware compatibility matrix
repeated power-cycle campaign
network outage recovery campaign
pool outage recovery campaign
long unattended soak
complete SBOM
formal production support policy
stable release guarantee
```

Details: [Known Limits](docs/KNOWN-LIMITS.md).


REPOSITORY STATUS
-----------------

```text
Current milestone: 0.0.4-alpha.25
Release tag:       v0.0.4-alpha.25
Runtime source:    ba02eb7429683550512b703cd4646d4d9ee6a888
Development:       paused
Release class:     Alpha preview
Production-ready:  no
```

Documentation index: [docs/README.md](docs/README.md).
