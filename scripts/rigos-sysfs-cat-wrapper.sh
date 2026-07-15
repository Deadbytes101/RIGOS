#!/bin/bash
set -euo pipefail

die() {
    printf 'rigos-sysfs-cat-wrapper: %s\n' "$*" >&2
    exit 1
}

real_cat="${RIGOS_REAL_CAT:-}"
retry_attempts="${RIGOS_SYSFS_RETRY_ATTEMPTS:-100}"

[[ -n "$real_cat" ]] ||
    die 'RIGOS_REAL_CAT is not set'

[[ "$real_cat" == /* ]] ||
    die 'real cat path is not absolute'

[[ -x "$real_cat" ]] ||
    die "real cat is not executable: $real_cat"

wrapper_path="$(readlink -f "$0")"
real_path="$(readlink -f "$real_cat")"

[[ "$wrapper_path" != "$real_path" ]] ||
    die 'recursive cat wrapper configuration'

[[ "$retry_attempts" =~ ^[0-9]+$ ]] ||
    die "invalid retry attempt count: $retry_attempts"

((retry_attempts >= 1 && retry_attempts <= 1000)) ||
    die "retry attempt count is out of range: $retry_attempts"

if [[ "$#" -eq 1 &&
    "$1" == /sys/class/block/loop*p[1-4]/dev ]]; then
    sysfs_device="$1"

    for ((attempt = 1; attempt <= retry_attempts; attempt++)); do
        if device_number="$("$real_cat" -- "$sysfs_device" 2>/dev/null)"; then
            if [[ "$device_number" =~ ^[0-9]+:[0-9]+$ ]]; then
                if ((attempt > 1)); then
                    printf \
                        'RIGOS partition sysfs device ready after %s attempts: %s\n' \
                        "$attempt" \
                        "$sysfs_device" \
                        >&2
                fi

                printf '%s\n' "$device_number"
                exit 0
            fi
        fi

        if ((attempt < retry_attempts)); then
            sleep 0.1
        fi
    done

    die \
        "partition sysfs device did not become readable after $retry_attempts attempts: $sysfs_device"
fi

exec "$real_cat" "$@"
