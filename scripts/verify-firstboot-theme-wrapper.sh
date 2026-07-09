#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
wrapper="$root/build/usb/includes.chroot/usr/lib/rigos/rigos-firstboot-whiptail"

test -f "$wrapper"

tmp=$(mktemp -d)
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT HUP INT TERM

backend="$tmp/fake-whiptail"
capture="$tmp/arguments.txt"
expected="$tmp/expected.txt"

cat >"$backend" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" >"$RIGOS_THEME_CAPTURE"
exit "${RIGOS_THEME_EXIT:-0}"
EOF
chmod 0755 "$backend"

set +e
RIGOS_WHIPTAIL_REAL="$backend" \
RIGOS_THEME_CAPTURE="$capture" \
RIGOS_THEME_EXIT=7 \
RIGOS_FIRSTBOOT_BACKTITLE='TEST BACKTITLE' \
sh "$wrapper" \
  --title 'RIGOS FIRST BOOT' \
  --menu 'Select Flight Sheet' \
  20 76 2 \
  manual 'Configure manually' \
  none 'Leave mining unconfigured'
status=$?
set -e

if [[ "$status" -ne 7 ]]; then
  echo "firstboot theme wrapper did not preserve backend exit status: $status" >&2
  exit 1
fi

cat >"$expected" <<'EOF'
--backtitle
TEST BACKTITLE
--ok-button
SELECT
--cancel-button
BACK
--title
RIGOS FIRST BOOT
--menu
Select Flight Sheet
20
76
2
manual
Configure manually
none
Leave mining unconfigured
EOF

if ! cmp -s "$expected" "$capture"; then
  echo "firstboot theme wrapper changed argument order or values" >&2
  diff -u "$expected" "$capture" >&2 || true
  exit 1
fi

set +e
RIGOS_WHIPTAIL_REAL="$tmp/missing-backend" sh "$wrapper" --msgbox test 8 40 >/dev/null 2>&1
missing_status=$?
set -e

if [[ "$missing_status" -ne 127 ]]; then
  echo "firstboot theme wrapper did not fail closed for a missing backend: $missing_status" >&2
  exit 1
fi

echo "RIGOS firstboot theme wrapper verification passed"
