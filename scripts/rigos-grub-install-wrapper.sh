#!/bin/bash
set -euo pipefail

die() {
    printf 'rigos-grub-install-wrapper: %s\n' "$*" >&2
    exit 1
}

real_grub_install="${RIGOS_REAL_GRUB_INSTALL:-}"

[[ -n "$real_grub_install" ]] ||
    die 'RIGOS_REAL_GRUB_INSTALL is not set'

[[ "$real_grub_install" == /* ]] ||
    die 'real grub-install path is not absolute'

[[ -x "$real_grub_install" ]] ||
    die "real grub-install is not executable: $real_grub_install"

wrapper_path="$(readlink -f "$0")"
real_path="$(readlink -f "$real_grub_install")"

[[ "$wrapper_path" != "$real_path" ]] ||
    die 'recursive grub-install wrapper configuration'

target=""
expect_target_value=0
caller_supplied_modules=0

for argument in "$@"; do
    if [[ "$expect_target_value" -eq 1 ]]; then
        target="$argument"
        expect_target_value=0
        continue
    fi

    case "$argument" in
        --target)
            expect_target_value=1
            ;;
        --target=*)
            target="${argument#--target=}"
            ;;
        --modules|--modules=*)
            caller_supplied_modules=1
            ;;
    esac
done

[[ "$expect_target_value" -eq 0 ]] ||
    die '--target requires a value'

if [[ "$target" == "i386-pc" ]]; then
    [[ "$caller_supplied_modules" -eq 0 ]] ||
        die 'caller supplied a conflicting BIOS module list'

    bios_modules='part_msdos ext2 search search_fs_uuid normal configfile'

    printf 'RIGOS BIOS GRUB embedded modules: %s\n' "$bios_modules"

    exec "$real_grub_install" \
        "--modules=$bios_modules" \
        "$@"
fi

exec "$real_grub_install" "$@"
