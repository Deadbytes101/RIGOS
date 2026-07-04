#!/usr/bin/env bash
set -euo pipefail
die() { printf 'collect-validation: %s\n' "$*" >&2; exit 1; }
run_id=""; binary=""; output=""; phase=""; xmrig=""; config=""; probe_helper=""
while [[ $# -gt 0 ]]; do case "$1" in
  --run-id) run_id="${2:-}"; shift 2;; --binary) binary="${2:-}"; shift 2;;
  --output) output="${2:-}"; shift 2;; --phase) phase="${2:-}"; shift 2;;
  --xmrig) xmrig="${2:-}"; shift 2;; --config) config="${2:-}"; shift 2;;
  --probe-helper) probe_helper="${2:-}"; shift 2;; *) die "unknown argument: $1";; esac; done
[[ "$run_id" =~ ^v0\.0\.1-rc[1-9][0-9]*-rig[0-9][0-9]-[0-9]{8}T[0-9]{6}Z$ ]] || die "invalid run ID"
[[ -x "$binary" ]] || die "binary is not executable"; [[ -n "$output" ]] || die "output is required"
[[ "$phase" =~ ^(baseline|miner-stopped|miner-running-no-api|miner-running-loopback-api|probe-timeout|finalize)$ ]] || die "invalid phase"
umask 077; mkdir -p "$output"/{inventory,inspection,mutation,verification,raw-meta}
case "$phase" in
  baseline) cp /etc/os-release "$output/inventory/os-release.txt"; uname -a > "$output/inventory/uname.txt"; lscpu > "$output/inventory/lscpu.txt";
    ldd "$binary" > "$output/inventory/runtime-libraries.txt"; sha256sum "$binary" > "$output/raw-meta/rigosd.sha256"; "$binary" --version > "$output/raw-meta/rigosd-version.txt"
    printf '{"hostname":"%s","username":"%s","home":"%s"}\n' "$(hostname)" "$(id -un)" "$HOME" > "$output/raw-meta/privacy.json"
    ps -eo pid,ppid,pgid,sid,lstart,args > "$output/mutation/before-processes.txt"
    [[ -z "$xmrig" || -z "$config" ]] || sha256sum "$xmrig" "$config" > "$output/mutation/before.sha256";;
  miner-stopped) "$binary" machine inspect --json > "$output/inspection/machine-inspect.json"; "$binary" miner inspect --json > "$output/inspection/miner-stopped.json";;
  miner-running-no-api) "$binary" miner inspect --json > "$output/inspection/miner-running-no-api.json";;
  miner-running-loopback-api) "$binary" miner inspect --json > "$output/inspection/miner-running-loopback-api.json"; "$binary" doctor --json > "$output/inspection/doctor.json";;
  probe-timeout) [[ -x "$probe_helper" ]] || die "probe helper is required"; "$binary" --xmrig-executable "$probe_helper" miner inspect --json > "$output/verification/probe-timeout.json";
    ps -eo pid,ppid,pgid,sid,lstart,args > "$output/verification/probe-processes-after.txt";;
  finalize) ps -eo pid,ppid,pgid,sid,lstart,args > "$output/mutation/after-processes.txt"
    if [[ -n "$xmrig" && -n "$config" ]]; then sha256sum "$xmrig" "$config" > "$output/mutation/after.sha256"; diff -u "$output/mutation/before.sha256" "$output/mutation/after.sha256" > "$output/mutation/comparison.txt" || die "persistent mutation detected"; fi
    printf '{"run_id":"%s","collected_at":"%s"}\n' "$run_id" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$output/raw-meta/raw-manifest.json"
    cat > "$output/raw-meta/result-input.json" <<EOF
{"schema":"dbyte.rigos.physical-validation-result/v1","run_id":"$run_id","overall":"blocked","checks":[
{"id":"binary.sha256_matches_authoritative_rc","result":"blocked"},{"id":"runtime.no_illegal_instruction","result":"blocked"},
{"id":"machine.real_hwmon_observed","result":"blocked"},{"id":"machine.huge_pages_observed","result":"blocked"},
{"id":"miner.stopped_snapshot_valid","result":"blocked"},{"id":"miner.running_without_api_snapshot_valid","result":"blocked"},
{"id":"miner.loopback_api_snapshot_valid","result":"blocked"},{"id":"inspection.zero_persistent_mutation","result":"blocked"},
{"id":"output.no_secret_leak","result":"blocked"}]}
EOF
    (cd "$output" && find . -type f ! -path './raw-meta/RAW-SHA256SUMS' -print0 | sort -z | xargs -0 sha256sum > raw-meta/RAW-SHA256SUMS)
    ;;
esac
printf 'phase %s complete for %s\n' "$phase" "$run_id"
