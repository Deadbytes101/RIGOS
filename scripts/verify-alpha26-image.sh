#!/usr/bin/env bash
set -euo pipefail

die(){ printf 'verify-alpha26-image: %s\n' "$*" >&2; exit 1; }
[[ $# -eq 2 ]] || die 'usage: verify-alpha26-image.sh <image> <manifest>'
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
image="$(readlink -f "$1")"
manifest="$(readlink -f "$2")"
[[ -f "$image" && -f "$manifest" ]] || die 'image or manifest is missing'

"$script_dir/verify-usb-appliance.sh" "$image" "$manifest"
[[ "$(jq -r .image_version "$manifest")" == '0.0.4-alpha.26' ]] || die 'not an Alpha.26 image'
[[ "$(jq -r .source_commit "$manifest")" =~ ^[0-9a-f]{40}$ ]] || die 'invalid source commit'

loop="$(losetup --find --show --read-only --offset $((526336 * 512)) --sizelimit $((2097152 * 512)) "$image")"
temporary="$(mktemp -d)"
cleanup(){ set +e; mountpoint -q "$temporary/root-a" && umount "$temporary/root-a"; losetup -d "$loop" 2>/dev/null; rm -rf "$temporary"; }
trap cleanup EXIT
mkdir -p "$temporary/root-a" "$temporary/root"
mount -o ro "$loop" "$temporary/root-a"
squashfs="$temporary/root-a/live/filesystem.squashfs"
[[ -f "$squashfs" ]] || die 'ROOT_A squashfs is missing'

listing="$temporary/listing.txt"
unsquashfs -ll "$squashfs" >"$listing"
for forbidden in \
  'squashfs-root/var/lib/rigos/status-agent/config.env' \
  'squashfs-root/var/lib/rigos/status-agent/ingest.secret' \
  'squashfs-root/var/lib/rigos/status-agent/source-id' \
  'squashfs-root/var/lib/rigos/status-agent/last-send.json' \
  'squashfs-root/etc/systemd/system/timers.target.wants/rigos-status-agent.timer'
do
  if awk '{print $NF}' "$listing" | grep -Fx "$forbidden" >/dev/null; then
    die "image contains forbidden baked status-agent state: $forbidden"
  fi
done

unsquashfs -no-progress -d "$temporary/root" "$squashfs" \
  etc/rigos-release \
  etc/systemd/system/rigos-status-agent.service \
  etc/systemd/system/rigos-status-agent.timer \
  usr/lib/rigos/rigos-status-agent \
  usr/local/bin/rig-status-agent \
  usr/share/doc/rigos/status-agent.txt \
  usr/share/rigos/status-agent.env.example >/dev/null

root="$temporary/root"
agent="$root/usr/lib/rigos/rigos-status-agent"
operator="$root/usr/local/bin/rig-status-agent"
service="$root/etc/systemd/system/rigos-status-agent.service"
timer="$root/etc/systemd/system/rigos-status-agent.timer"
[[ -x "$agent" && -x "$operator" ]] || die 'status-agent entrypoint is not executable'
[[ "$(stat -c '%a' "$agent")" == 755 ]] || die 'status agent mode is not 755'
[[ "$(stat -c '%a' "$operator")" == 755 ]] || die 'operator command mode is not 755'
python3 -m py_compile "$agent" "$operator"

grep -Fqx 'VERSION_ID="0.0.4-alpha.26"' "$root/etc/rigos-release" || die 'embedded release version mismatch'
grep -Fqx 'ConditionPathExists=/var/lib/rigos/status-agent/config.env' "$service" || die 'configuration condition missing'
grep -Fqx 'ConditionPathExists=/var/lib/rigos/status-agent/ingest.secret' "$service" || die 'secret condition missing'
grep -Fqx 'SuccessExitStatus=75 76' "$service" || die 'bounded observer exit contract missing'
grep -Fqx 'ReadWritePaths=/var/lib/rigos/status-agent' "$service" || die 'persistent write boundary missing'
grep -Fqx 'OnUnitActiveSec=30s' "$timer" || die 'timer cadence mismatch'
if grep -Eq '(^|[ =])(Requires|Before)=.*rigos-miner\.service' "$service"; then
  die 'status agent is coupled to mining authority'
fi
for required in \
  'OBSERVATION_SCHEMA = "rigos.status-observation/v1"' \
  'COMPONENT_IDS = (' \
  'X-RigOS-Timestamp' \
  'X-RigOS-Nonce' \
  'X-RigOS-Signature' \
  'return 75' \
  'return 76'
do
  grep -Fq "$required" "$agent" || die "agent contract missing: $required"
done
if grep -Eq '/run/rigos/xmrig|/var/lib/rigos/current|pool_endpoint|hashrate_hs|accepted_shares|rejected_shares' "$agent"; then
  die 'agent reads forbidden private mining runtime data'
fi

echo 'RIGOS Alpha.26 status-agent image verification passed'
