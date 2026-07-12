RIGOS PROJECT HISTORY
=====================

This history records engineering milestones. It does not imply that
every intermediate Alpha was public, stable or production-ready.


INITIAL USB APPLIANCE WORK
--------------------------

RIGOS moved from observation-only tooling toward a bootable USB
appliance with BIOS and UEFI paths, immutable roots and a separate
persistent state partition.


PERSISTENT STATE AND RECOVERY
-----------------------------

The appliance added exact USB state proof, state growth, repair paths,
recovery access, SSH host identity and state readiness gates.


FIRSTBOOT TTY WORK
------------------

The firstboot UI was moved onto the physical console with correct stream
handling so whiptail could draw on tty1 while answers were captured
separately.


SYSTEMD TRANSACTION DEFECT
--------------------------

Physical testing found that firstboot could be removed from the initial
boot transaction by a conflict with utility mode before conditions were
evaluated. The fix made kernel command-line conditions the mode
authority.


ALPHA.22
--------

Alpha.22 repaired the physical firstboot/systemd transaction defect and
became the first functional physical preview.


ALPHA.23 AND ALPHA.24
---------------------

The next hardening work added miner health observation, bounded
supervision, network inspection, hostname synchronization and a namespace
fix for miner health persistent state.


ALPHA.25
--------

Alpha.25 added the short `rig` operator command, explicit operator JSON
allowlists, automation-friendly status, preserved runtime gates and
recorded physical health evidence.


CURRENT STATUS
--------------

Alpha.25 is the frozen functional milestone. Development is paused.
The project is usable as an experimental appliance but is not declared
stable or production-ready.
