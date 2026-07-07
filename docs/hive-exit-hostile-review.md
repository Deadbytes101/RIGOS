HIVE EXIT HOSTILE REVIEW
========================

The first stability batch passed source verification, but review found two
release blockers before image construction.

1. The inspector must never select the identity-bearing runtime configuration
   when the public configuration was explicitly requested.

2. The public XMRig view must be constructed from an allowlist. Copying the
   private configuration and deleting known fields is not a security boundary.

CURRENT BOUNDARY
----------------

The legacy renderer now runs only inside a private staging directory under
/run/rigos with mode 0700.  The runtime publication authority then creates a
new public document from an explicit jq allowlist and atomically publishes:

  /run/rigos/xmrig.json                 mode 0640
  /run/rigos/xmrig-public.json          mode 0644
  /run/rigos/runtime-config-status.json mode 0644

The status document is published last.  A failed publication therefore cannot
claim a new ready state before the private and public documents are in place.

The managed XMRig process uses the supported short option:

  xmrig -c /run/rigos/xmrig.json

RIGOS process discovery currently recognizes only the long --config forms.
The rigosd and rigosctl wrappers explicitly select xmrig-public.json, so the
managed process command line cannot override the public inspection path.
This compatibility boundary is covered by an integration test and must not be
changed independently from the parser or systemd wiring.

REQUIRED VERIFICATION
---------------------

  cargo fmt --all -- --check
  cargo test -p rigos-xmrig --test explicit_config_boundary
  cargo test -p rigos-config --test hive_exit_runtime_publish
  ./scripts/verify.sh

The publication fixture injects unknown sentinel fields at the top level and
inside CPU, pool, and HTTP objects.  Private runtime truth must retain them.
The public view must contain none of them.

GitHub Actions are disabled for this account.  Verification evidence therefore
comes from the local WSL source gate and exact artifact provenance, not from a
remote workflow badge.
