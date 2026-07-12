RIGOS ARCHITECTURE
==================

RIGOS 0.0.4-alpha.25 is a Debian 12 based x86_64 USB appliance. The
normal root is immutable. Durable operator state lives in a separate
persistent partition and is activated through explicit revisions.


BOOT MODES
----------

```text
normal
  configured state -> automatic activation and mining
  unconfigured     -> firstboot on tty1

utility
  local console utility
  firstboot suppressed by kernel command line condition
  normal mining not started by utility mode itself

recovery ISO
  stateless diagnostics and repair role
  not the normal appliance image
```


SYSTEMD FLOW
------------

Configured boot:

```text
rigos-state.service
  -> rigos-state-ready.service
  -> rigos-ssh-hostkeys.service
  -> rigos-profile-apply.service
  -> rigos-runtime-render.service
  -> rigos-hugepages.service
  -> rigos-miner.service
  -> rigos-miner-health.timer
```

Unconfigured boot still reaches firstboot on tty1. The miner remains
gated until configuration exists and activation succeeds.


PERSISTENT STATE
----------------

Persistent state stores configuration revisions, the current pointer,
SSH host identity, recovery credential records and miner health budget.
The appliance root is not the source of operator state.


CONFIGURATION REVISION AUTHORITY
--------------------------------

Configuration is validated before mutation. A revision is committed,
then the current pointer is switched atomically. Activation applies the
profile, runtime configuration and service policy from the current
revision.


RUNTIME GENERATION
------------------

Runtime files are generated under `/run/rigos`. Private miner config is
kept separate from public redacted inspection output. `rigosctl` remains
the detailed authority command; `rig` is the short operator surface.


HUGE PAGES
----------

The huge-page authority reads policy and runtime truth, writes kernel
huge-page state directly, then publishes visible status. Miner startup
requires the authority path.


XMRIG OWNERSHIP
---------------

XMRig is owned by `rigos-miner.service`. The service uses existing
systemd gates and does not start before persistent state, runtime config
and huge pages are ready.


HEALTH OBSERVER
---------------

The miner health observer reads systemd state, runtime revision,
network state, bounded journal evidence and the restricted loopback
XMRig HTTP API. It records non-secret status under `/run/rigos`.


RECOVERY BOUNDARIES
-------------------

Recovery is explicit. It does not silently destroy persistent state or
rewrite the last known-good configuration. Rollback behavior must be
invoked by an explicit recovery command when supported.
