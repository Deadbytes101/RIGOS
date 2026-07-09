#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
wrapper="$root/build/usb/includes.chroot/usr/lib/rigos/rigos-firstboot-whiptail"

if [[ ! -f "$wrapper" ]]; then
  echo "preview-firstboot-theme: wrapper is missing: $wrapper" >&2
  exit 1
fi

if [[ ! -x /usr/bin/whiptail ]]; then
  echo "preview-firstboot-theme: /usr/bin/whiptail is not installed" >&2
  exit 127
fi

RIGOS_WHIPTAIL_REAL=/usr/bin/whiptail \
RIGOS_FIRSTBOOT_BACKTITLE='RIGOS SYSTEM CONFIGURATION // LOCAL NODE SETUP // OFFLINE AUTHORITY' \
sh "$wrapper" \
  --title 'RIGOS FIRST BOOT' \
  --menu $'SELECT FLIGHT SHEET\n\nChoose a local mining configuration source.\n[UP/DOWN] MOVE   [ENTER] SELECT   [ESC] BACK' \
  20 76 6 \
  manual 'Configure this node manually' \
  none 'Leave mining unconfigured' \
  native:xmr 'Use native XMR flight sheet' \
  import:legacy 'Import an external flight sheet'
