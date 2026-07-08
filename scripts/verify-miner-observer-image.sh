#!/bin/bash
set -euo pipefail

die() {
    printf 'verify-miner-observer-image: %s\n' "$*" >&2
    exit 1
}

[[ $# -eq 1 ]] || die 'usage: verify-miner-observer-image.sh <image>'
image="$(readlink -f "$1")"
[[ -f "$image" ]] || die "image is missing: $image"
[[ "$(id -u)" -eq 0 ]] || die 'must run as root'

partition_json="$(sfdisk --json "$image")"
start="$(jq -r '.partitiontable.partitions[1].start' <<<"$partition_json")"
size="$(jq -r '.partitiontable.partitions[1].size' <<<"$partition_json")"
[[ "$start" =~ ^[0-9]+$ && "$size" =~ ^[0-9]+$ ]] || die 'ROOT_A geometry is invalid'

loop="$(losetup --find --show --read-only --offset $((start * 512)) --sizelimit $((size * 512)) "$image")"
temporary="$(mktemp -d)"
cleanup() {
    set +e
    mountpoint -q "$temporary/root-a" && umount "$temporary/root-a"
    losetup -d "$loop" 2>/dev/null
    rm -rf "$temporary"
}
trap cleanup EXIT

mkdir -p "$temporary/root-a" "$temporary/squash"
mount -o ro "$loop" "$temporary/root-a"
squashfs="$temporary/root-a/live/filesystem.squashfs"
[[ -f "$squashfs" ]] || die 'ROOT_A squashfs is missing'

unsquashfs -no-progress -d "$temporary/squash" "$squashfs" \
    usr/bin/python3 usr/bin/python3.11 \
    usr/lib/rigos/rigos-runtime-render \
    usr/lib/rigos/rigos-runtime-publish \
    usr/lib/rigos/rigos-miner-health \
    etc/systemd/system/rigos-miner-health.service \
    etc/systemd/system/rigos-miner-health.timer \
    etc/systemd/system/rigos-runtime-render.service \
    >/dev/null

root="$temporary/squash"
renderer="$root/usr/lib/rigos/rigos-runtime-render"
publisher="$root/usr/lib/rigos/rigos-runtime-publish"
observer="$root/usr/lib/rigos/rigos-miner-health"
service="$root/etc/systemd/system/rigos-miner-health.service"
timer="$root/etc/systemd/system/rigos-miner-health.timer"

[[ -x "$root/usr/bin/python3" ]] || die 'Python runtime is missing'
[[ -x "$renderer" ]] || die 'runtime renderer is missing or not executable'
[[ -x "$publisher" ]] || die 'runtime publisher is missing or not executable'
[[ -x "$observer" ]] || die 'miner observer is missing or not executable'
python3 -m py_compile "$renderer" "$observer"

python3 - "$observer" <<'PY'
import importlib.machinery
import importlib.util
import sys

path = sys.argv[1]
loader = importlib.machinery.SourceFileLoader("rigos_image_observer", path)
spec = importlib.util.spec_from_loader("rigos_image_observer", loader)
if spec is None:
    raise SystemExit("could not load extracted observer")
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)

properties = {"ActiveState": "active", "SubState": "running", "MainPID": "123"}

def classify(summary):
    metrics = module.summary_metrics(summary)
    return metrics, module.classify(
        properties,
        "S",
        600,
        "r1",
        "ready",
        "r1",
        metrics,
        None,
        "",
        True,
    )

stale_metrics, stale_state = classify({
    "hashrate": {"total": [0, 340.8, 339.7], "highest": 341.2},
    "connection": {
        "pool": "pool.example:1234",
        "ip": None,
        "uptime": 0,
        "uptime_ms": 0,
        "failures": 3,
    },
})
if stale_metrics.get("pool_connected") is not False:
    raise SystemExit("extracted observer trusts stale pool name")
if stale_state != ("waiting_external", "pool_or_network_unavailable"):
    raise SystemExit(f"extracted observer misclassifies disconnected historical hashrate: {stale_state}")

historical_metrics, historical_state = classify({
    "hashrate": {"total": [0, 340.9, 340.7], "highest": 342.1},
    "connection": {
        "pool": "pool.example:1234",
        "ip": "203.0.113.10",
        "uptime": 590,
        "uptime_ms": 590125,
        "failures": 0,
    },
})
if historical_metrics.get("pool_connected") is not True:
    raise SystemExit("extracted observer rejects active pool evidence")
if historical_state != ("degraded", "no_current_hashrate_from_api"):
    raise SystemExit(f"extracted observer trusts historical hashrate as current: {historical_state}")

active_metrics, active_state = classify({
    "hashrate": {"total": [341.2, 340.9, 340.7], "highest": 342.1},
    "connection": {
        "pool": "pool.example:1234",
        "ip": "203.0.113.10",
        "uptime": 590,
        "uptime_ms": 590125,
        "failures": 0,
    },
})
if active_metrics.get("pool_connected") is not True:
    raise SystemExit("extracted observer rejects active pool evidence")
if active_state != ("ready", None):
    raise SystemExit(f"extracted observer rejects active hashrate: {active_state}")

schema_metrics = module.summary_metrics({
    "connection": {
        "accepted": 43.9,
        "rejected": 1.2,
        "failures": 2.5,
        "ping": 109.7,
    },
    "results": {
        "shares_good": 42.8,
        "shares_total": 44.1,
    },
    "hugepages": [1168.5, 1169.5],
})
for key in (
    "accepted_shares",
    "rejected_shares",
    "connection_failures",
    "pool_ping_ms",
    "hugepages_used",
    "hugepages_total",
):
    if schema_metrics.get(key) is not None:
        raise SystemExit(f"extracted observer truncates fractional counters: {key}")

old_ready_new_external = "\n".join([
    "miner    speed 10s/60s/15m 341.2 340.9 340.7 H/s",
    "cpu accepted (43/0) diff 10000",
    "net connect error: operation timed out",
])
state = module.journal_fallback_state(old_ready_new_external, 600, "api_unavailable")
if state != ("waiting_external", "pool_or_network_unavailable"):
    raise SystemExit(f"extracted observer trusts stale journal ready evidence: {state}")

old_external_new_ready = "\n".join([
    "net connect error: operation timed out",
    "miner    speed 10s/60s/15m 341.2 340.9 340.7 H/s",
    "cpu accepted (44/0) diff 10000",
])
state = module.journal_fallback_state(old_external_new_ready, 600, "api_unavailable")
if state != ("ready", None):
    raise SystemExit(f"extracted observer ignores newer journal ready evidence: {state}")

authority_error_state = module.classify(
    properties,
    "S",
    600,
    "r1",
    "ready",
    "r1",
    None,
    "api_token_missing",
    "cpu accepted (43/0) diff 10000",
    True,
)
if authority_error_state != ("degraded", "api_token_missing"):
    raise SystemExit(f"extracted observer hides API authority failure: {authority_error_state}")

transient_state = module.classify(
    properties,
    "S",
    600,
    "r1",
    "ready",
    "r1",
    None,
    "api_unavailable",
    "cpu accepted (43/0) diff 10000",
    True,
)
if transient_state != ("ready", None):
    raise SystemExit(f"extracted observer rejects bounded transient fallback: {transient_state}")
PY

for required in \
    'API_TOKEN = Path(os.environ.get("RIGOS_XMRIG_API_TOKEN_PATH", str(RUNTIME / "xmrig-api-token")))' \
    'API_HOST = "127.0.0.1"' \
    'API_PORT = 18080' \
    'token = secrets.token_urlsafe(48)' \
    '"access-token": api_token' \
    '"restricted": True' \
    'http.pop("access-token", None)' \
    '"token_path": str(API_TOKEN)'
do
    grep -Fq "$required" "$renderer" || die "runtime API authority contract is missing: $required"
done
if grep -Eq 'API_HOST = "(0\.0\.0\.0|::)"' "$renderer"; then
    die 'runtime API authority exposes a non-loopback bind'
fi

for required in \
    'RIGOS_RUNTIME_PATH="$stage" \' \
    'RIGOS_XMRIG_API_TOKEN_PATH="$runtime/xmrig-api-token" \' \
    '    "$renderer"'
do
    grep -Fqx "$required" "$publisher" || die "runtime token publication contract is missing: $required"
done

for required in \
    'connection.request(' \
    '"/2/summary"' \
    '"Authorization": f"Bearer {token}"' \
    'API_MAX_BYTES = 256 * 1024' \
    'def nonnegative_integer(value: object) -> int | None:' \
    'connection_ip = connection.get("ip")' \
    'connection_uptime_ms = nonnegative_number(connection.get("uptime_ms"))' \
    '"pool_connected": pool_connected' \
    'current_hashrate = api_metrics.get("hashrate_10s")' \
    'if pool_connected and current_hashrate_positive:' \
    'return "degraded", "current_hashrate_unavailable"' \
    'return "degraded", "no_current_hashrate_from_api"' \
    'if api_error not in (None, "api_unavailable"):' \
    'def latest_journal_signal(text: str) -> str | None:' \
    '"latest_journal_signal": latest_journal_signal(journal)' \
    '"source": "xmrig_http_api" if metrics is not None else "journal_fallback"' \
    'return "degraded", api_error or "miner_api_unavailable"'
do
    grep -Fq "$required" "$observer" || die "miner observer API contract is missing: $required"
done

for required in \
    'ExecStart=/usr/bin/python3 /usr/lib/rigos/rigos-miner-health' \
    'RestrictAddressFamilies=AF_UNIX AF_INET' \
    'IPAddressDeny=any' \
    'IPAddressAllow=127.0.0.0/8' \
    'ReadWritePaths=/run/rigos'
do
    grep -Fqx "$required" "$service" || die "miner observer sandbox contract is missing: $required"
done
grep -Fq 'OnUnitActiveSec=1min' "$timer" || die 'miner observer timer cadence is missing'
grep -Fq 'ExecStart=/usr/lib/rigos/rigos-runtime-authority' "$root/etc/systemd/system/rigos-runtime-render.service" \
    || die 'serialized runtime authority is not wired'

printf 'RIGOS authenticated XMRig observer image verification passed: %s\n' "$image"
