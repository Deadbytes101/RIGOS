RIGOS MERMAID DIAGRAMS
=======================

These Mermaid blocks are the editable architecture source for the static SVG
figures published on the RIGOS website.

The browser does not execute Mermaid or JavaScript. The website serves checked-in
SVG files so the diagrams remain readable without a client runtime, package
manager, CDN or framework.


OPERATING PATH
--------------

```mermaid
flowchart TD
    BOOT["BIOS or UEFI"] --> ROOT["Immutable Debian 12 root"]
    ROOT -. "utility mode" .-> UTIL["Local utility console"]
    ROOT -. "recovery boot" .-> RECOVERY["Stateless recovery ISO"]
    ROOT --> STATE["Verified persistent state"]
    STATE --> REVISION["Committed configuration revision"]
    REVISION --> PROFILE["Machine profile applied"]
    PROFILE --> RUNTIME["Private runtime rendered under /run/rigos"]
    RUNTIME --> HUGEPAGES["Huge pages ready"]
    RUNTIME --> NETWORK["Network ready"]
    HUGEPAGES --> MINER["systemd-owned XMRig"]
    NETWORK --> MINER
    MINER --> OBSERVER["Bounded health observer"]
    OBSERVER --> OPS["rig and rigosctl"]
```


SYSTEMD ORDERING
----------------

```mermaid
flowchart TD
    STATE["rigos-state.service"] --> READY["rigos-state-ready.service"]
    READY --> HOSTKEYS["rigos-ssh-hostkeys.service"]
    HOSTKEYS --> PROFILE["rigos-profile-apply.service"]
    PROFILE --> RUNTIME["rigos-runtime-render.service"]
    RUNTIME --> HUGEPAGES["rigos-hugepages.service"]
    HUGEPAGES --> MINER["rigos-miner.service"]
    MINER --> HEALTH["rigos-miner-health.timer"]
```


CONFIGURATION REVISION TRANSACTION
----------------------------------

```mermaid
flowchart TD
    INPUT["Operator input"] --> VALIDATE{"Complete candidate valid?"}
    VALIDATE -- "no" --> REJECT["Reject without mutation"]
    VALIDATE -- "yes" --> CREATE["Create immutable revision"]
    CREATE --> COMMIT["Commit revision"]
    COMMIT --> SWITCH["Atomically switch current pointer"]
    SWITCH --> APPLY["Apply machine profile"]
    APPLY --> RENDER["Render private runtime configuration"]
    RENDER --> PUBLIC["Publish redacted status"]
    PUBLIC --> ACTIVATE["Permit service activation"]
```


STATIC PUBLICATION BOUNDARY
---------------------------

```text
site/DIAGRAMS.md              editable Mermaid source rendered by GitHub
site/diagrams/*.svg           static website figures
site/architecture.html        semantic text and figure references
browser JavaScript            none
runtime Mermaid dependency    none
```
