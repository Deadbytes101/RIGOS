#!/bin/bash
set -euo pipefail

die() {
    printf 'rigos-blockdev-wrapper: %s\n' "$*" >&2
    exit 1
}

real_blockdev="${RIGOS_REAL_BLOCKDEV:-}"
retry_attempts="${RIGOS_BLOCKDEV_RETRY_ATTEMPTS:-100}"

[[ -n "$real_blockdev" ]] ||
    die 'RIGOS_REAL_BLOCKDEV is not set'

[[ "$real_blockdev" == /* ]] ||
    die 'real blockdev path is not absolute'

[[ -x "$real_blockdev" ]] ||
    die "real blockdev is not executable: $real_blockdev"

wrapper_path="$(readlink -f "$0")"
real_path="$(readlink -f "$real_blockdev")"

[[ "$wrapper_path" != "$real_path" ]] ||
    die 'recursive blockdev wrapper configuration'

[[ "$retry_attempts" =~ ^[0-9]+$ ]] ||
    die "invalid retry attempt count: $retry_attempts"

((retry_attempts >= 1 && retry_attempts <= 1000)) ||
    die "retry attempt count is out of range: $retry_attempts"

if [[ "$#" -eq 2 &&
    "$1" == "--getsize64" &&
    "$2" == /work/rigos-appliance/partition-nodes.*/*p[1-4] ]]; then
    node="$2"

    for ((attempt = 1; attempt <= retry_attempts; attempt++)); do
        if size="$("$real_blockdev" "$@" 2>/dev/null)"; then
            if [[ "$size" =~ ^[0-9]+$ ]] && ((size > 0)); then
                if ((attempt > 1)); then
                    printf \
                        'RIGOS partition node ready after %s attempts: %s\n' \
                        "$attempt" \
                        "$node" \
                        >&2
                fi

                printf '%s\n' "$size"
                exit 0
            fi
        fi

        if ((attempt < retry_attempts)); then
            sleep 0.1
        fi
    done

    die \
        "partition block device did not become readable after $retry_attempts attempts: $node"
fi

exec "$real_blockdev" "$@"
