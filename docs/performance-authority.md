# Alpha.6 performance authority

RIGOS owns machine-wide huge page policy locally. It does not infer success
from configuration intent or from a successful write. The kernel read-back is
the authoritative result.

The boot order is:

```text
rigos-state.service
rigos-recovery-access.service
rigos-state-ready.service
rigos-profile-apply.service
rigos-hugepages.service
rigos-miner.service
```

For `rx/0`, the policy target is 1280 pages of 2 MiB with a 1 GiB
`MemAvailable` reserve. If the full target is unsafe, RIGOS computes a safe
partial attempt while retaining 1280 as policy intent. Partial or unavailable
allocation is visible degradation and does not block mining.

The authoritative runtime record is:

```text
/run/rigos/performance-status.json
```

It uses `rigos.performance-status/v1` and records the boot ID, config revision,
algorithm, target, safe attempt, actual kernel pages, memory inputs and a stable
status. `rigosctl doctor` rejects records from another boot or config revision.
On a fresh node without a `current` revision it records `not_provisioned`, reads
the current kernel allocation and exits successfully without writing huge-page
state or controlling the miner. A broken target behind an existing `current`
pointer remains a hard failure.

## Lifecycle gate

`/run/rigos` is a shared root-owned tmpfiles directory. Services may publish
atomic status files there but do not own or remove the shared parent. Persistent
state is accepted only after `rigos-state-ready` matches the current boot ID,
attested PARTUUID and major:minor, ext4 label, mount source and the complete
`rw,nosuid,nodev,noexec,noatime` option set.

Local `rigosadmin` password establishment is a separate tty1 recovery phase.
It does not create a revision, alter mining configuration or enable remote
access. A state failure leaves tty1 diagnostics available while profile,
firstboot, huge pages and miner remain gated.

Configuration commit and activation are separate. A successful commit creates
one durable revision. Activation failure preserves that revision, records
`activation_failed`, leaves the miner stopped and retries only activation on
the next invocation. `ready` is tied to the current revision.

On a configured physical node, run the packaged lifecycle stress gate:

```bash
sudo /usr/lib/rigos/rigos-lifecycle-cycles 20
```

It must retain the same current pointer and revision count, preserve
`/run/rigos`, avoid `226/NAMESPACE`, and complete all profile, huge-page and
miner restart cycles.

Expected degraded states return success so the miner may continue. Unreadable
configuration, unverifiable machine truth or failure to atomically publish and
read back status is a hard service failure and blocks miner startup.

The authority writes `/proc/sys/vm/nr_hugepages` directly. It does not execute
`sysctl`, match CPU model names, inspect internal filesystems or change the
Flight Sheet v1 schema.

## Physical gate

On the test node, capture:

```bash
cat /run/rigos/performance-status.json
cat /proc/sys/vm/nr_hugepages
grep -E 'MemAvailable|HugePages_Total|HugePages_Free|Hugepagesize' /proc/meminfo
systemctl show rigos-hugepages.service rigos-miner.service -p ActiveState -p SubState
journalctl -b -u rigos-miner.service --no-pager | grep -i 'huge pages'
rigosctl doctor --json
```

The gate requires current status matching kernel read-back, XMRig huge-page use
greater than zero, accepted shares with zero rejected shares, successful
reapplication after reboot and unchanged internal-disk layout.
Acceptance evidence must come from a fully wiped and reflashed USB; revisions
created during manual recovery are not valid evidence.

## Reserved Alpha.6 interfaces

Later Alpha.6 stages extend this authority without changing its status meaning:

- native hardware telemetry extends the existing machine inspector and adds
  `rigosctl hardware inspect` while preserving `machine inspect`;
- deterministic CPU benchmark observes the existing miner every 10 seconds and
  writes `rigos.benchmark/v1` under `/var/lib/rigos/benchmarks` without changing
  pool, identity, algorithm, threads or service policy;
- XMRig metrics use an authenticated API bound only to `127.0.0.1:18080`; the
  system journal remains event truth for new-job and failure timestamps;
- the conservative watchdog requires multiple persistent fault signals, waits
  10 minutes between recovery attempts and permits at most three restarts per
  30 minutes;
- the final doctor adds boot, state, config, miner, pool, thermal and watchdog
  checks without reading or mounting internal filesystems.

These interfaces are architectural reservations only. Alpha.6 Huge Page
Authority does not enable the API, benchmark, watchdog or expanded telemetry.
