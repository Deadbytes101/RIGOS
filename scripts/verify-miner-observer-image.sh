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
    'connection_ip = connection.get("ip")' \
    'connection_uptime_ms = nonnegative_number(connection.get("uptime_ms"))' \
    '"pool_connected": pool_connected' \
    '"source": "xmrig_http_api" if metrics is not None else "journal_fallback"' \
    'return "degraded", "no_hashrate_from_api"' \
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
