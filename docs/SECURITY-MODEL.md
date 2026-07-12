RIGOS SECURITY MODEL
====================

RIGOS Alpha.25 is an Alpha preview. The security model is practical and
local-first, not a formal product security certification.


IMMUTABLE RELEASE AUTHORITY
---------------------------

The Alpha.25 runtime authority is:

```text
tag:    v0.0.4-alpha.25
source: ba02eb7429683550512b703cd4646d4d9ee6a888
```

Tags and release assets must not be moved or replaced.


PERSISTENT STATE TRUST BOUNDARY
-------------------------------

Persistent state is separate from the immutable root. RIGOS verifies the
state device before mounting and uses revision activation instead of
editing runtime files directly.


LOCAL RESTRICTED MINER API
--------------------------

XMRig observation uses a restricted loopback HTTP API. Token contents
are protected and must not appear in README evidence, status output or
logs.


REDACTED PUBLIC VIEWS
---------------------

Operator output is built from public allowlists. Raw private runtime
configuration, identities, token paths and private hashes are not
operator output.


NO HIDDEN SUDO
--------------

The short `rig` command never invokes sudo internally. Mutating actions
require explicit root intent from the operator.


EXPLICIT RECOVERY MUTATION
--------------------------

Read-only recovery status is the default. Any destructive or state
changing recovery action must use an explicit command.


SECRET HANDLING
---------------

Do not publish:

```text
wallet identity
password material
API token contents
SSH private keys
private rig.conf
private runtime config
persistent-state dumps
raw logs containing secrets
```


RELEASE HASH VERIFICATION
-------------------------

Release artifacts must be verified with SHA256 before use. Source
archives from GitHub are not bootable images.


ALPHA LIMITATIONS
-----------------

Alpha.25 does not yet have the full hardware matrix, power-cycle
campaign, network recovery campaign, pool outage campaign, long soak or
formal production support policy required for a stable release.
