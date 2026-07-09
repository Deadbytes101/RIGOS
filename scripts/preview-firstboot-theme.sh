#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
wrapper="$root/build/usb/includes.chroot/usr/lib/rigos/rigos-firstboot-whiptail"
mode=${1:-menu}

if [[ ! -f "$wrapper" ]]; then
  echo "preview-firstboot-theme: wrapper is missing: $wrapper" >&2
  exit 1
fi

if [[ ! -x /usr/bin/whiptail ]]; then
  echo "preview-firstboot-theme: /usr/bin/whiptail is not installed" >&2
  exit 127
fi

run_theme() {
  set +e
  RIGOS_WHIPTAIL_REAL=/usr/bin/whiptail \
  RIGOS_FIRSTBOOT_BACKTITLE='RIGOS SETUP UTILITY   LOCAL NODE CONFIGURATION' \
  sh "$wrapper" "$@"
  status=$?
  set -e
  printf 'RIGOS_THEME_PREVIEW_EXIT=%s\n' "$status"
}

preview_menu() {
  run_theme \
    --title 'FLIGHT SHEET SELECTION' \
    --menu $'SELECT FLIGHT SHEET\n\nChoose how this node should be configured.\n\n[UP/DOWN] MOVE   [ENTER] SELECT   [ESC] BACK' \
    20 76 6 \
    manual 'Configure this node manually' \
    none 'Leave mining unconfigured' \
    native:xmr 'Use native XMR flight sheet' \
    import:legacy 'Import an external flight sheet'
}

preview_confirm() {
  run_theme \
    --title 'COMMIT CONFIGURATION' \
    --yesno $'CONFIGURATION SUMMARY\n\nNode          rig01\nFlight Sheet  xmr\nAlgorithm     rx/0\nThreads       exact 2\nHuge Pages    enabled\nStart Policy  on boot\n\nApply this configuration to local persistent state?' \
    20 76
}

preview_input() {
  run_theme \
    --title 'NODE IDENTITY' \
    --inputbox $'NODE NAME\n\nEnter the local appliance name.\nAllowed: A-Z, a-z, 0-9 and hyphen.' \
    14 72 'rig01'
}

preview_message() {
  run_theme \
    --title 'SETUP COMPLETE' \
    --msgbox $'CONFIGURATION COMMITTED\n\nPersistent state verified.\nRuntime config published.\nMiner activation requested.\n\nThe local console will now return to the system.' \
    16 72
}

case "$mode" in
  menu)
    preview_menu
    ;;
  confirm)
    preview_confirm
    ;;
  input)
    preview_input
    ;;
  message)
    preview_message
    ;;
  all)
    preview_menu
    preview_input
    preview_confirm
    preview_message
    ;;
  -h|--help)
    cat <<'EOF'
usage: scripts/preview-firstboot-theme.sh [menu|input|confirm|message|all]

Displays the RIGOS firstboot theme without reading or changing persistent state.
EOF
    ;;
  *)
    echo "preview-firstboot-theme: unknown mode: $mode" >&2
    exit 64
    ;;
esac
