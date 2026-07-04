#!/usr/bin/env bash
set -euo pipefail
printf 'build-release.sh is compatibility-only; use build-rc.sh with an explicit RC identifier.\n' >&2
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/build-rc.sh" "$@"

