#!/usr/bin/env bash
set -euo pipefail

target=${1:-build/usb/includes.chroot}
if [[ ! -d "$target" ]]; then
  echo "runtime dependency scan target is missing: $target" >&2
  exit 66
fi

pattern='(^|[^[:alnum:]_])(curl|wget|Invoke-WebRequest|latest)([^[:alnum:]_]|$)'

set +e
rg -n -i -- "$pattern" "$target"
status=$?
set -e

case "$status" in
  0)
    echo "runtime miner download or floating dependency detected" >&2
    exit 1
    ;;
  1)
    exit 0
    ;;
  *)
    echo "runtime dependency scan failed: exit $status" >&2
    exit "$status"
    ;;
esac
