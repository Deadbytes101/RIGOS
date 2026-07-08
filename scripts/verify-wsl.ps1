[CmdletBinding()]
param(
    [string]$Repository,
    [string]$Distribution = $env:RIGOS_WSL_DISTRO
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

if ([string]::IsNullOrWhiteSpace($Repository)) {
    if ([string]::IsNullOrWhiteSpace($PSScriptRoot)) {
        throw "RIGOS_WSL_SCRIPT_ROOT_UNAVAILABLE"
    }
    $Repository = Split-Path -Parent $PSScriptRoot
}

$WslPrefix = @()
if (-not [string]::IsNullOrWhiteSpace($Distribution)) {
    $WslPrefix += @("-d", $Distribution)
}

$Repository = (Resolve-Path -LiteralPath $Repository -ErrorAction Stop).Path
$PathConverter = @'
set -euo pipefail
IFS= read -r windows_path
windows_path="${windows_path%$'\r'}"
wslpath -a "$windows_path"
'@

$SavedErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $LinuxRepoOutput = $Repository |
        & wsl.exe @WslPrefix -- bash -lc $PathConverter 2>&1
    $PathExitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $SavedErrorActionPreference
}

$LinuxRepoLines = @($LinuxRepoOutput | ForEach-Object { $_.ToString() })
if ($PathExitCode -ne 0) {
    throw "WSL_PATH_CONVERSION_FAILED: $($LinuxRepoLines -join ' | ')"
}

$LinuxRepo = ($LinuxRepoLines | Select-Object -Last 1).Trim()
if ([string]::IsNullOrWhiteSpace($LinuxRepo)) {
    throw "WSL_PATH_CONVERSION_EMPTY"
}

$Shell = @'
set -euo pipefail

repo="$1"
cd "$repo"

if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi

missing=0
for tool in cargo rustc python3 bash sh git grep rg mktemp; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'RIGOS_WSL_TOOL_MISSING=%s\n' "$tool" >&2
    missing=1
  fi
done

if [ "$missing" -ne 0 ]; then
  printf '%s\n' 'Install the missing tool inside this WSL distribution, then rerun scripts/verify-wsl.ps1.' >&2
  exit 127
fi

for component in fmt clippy; do
  if ! cargo "$component" --version >/dev/null 2>&1; then
    printf 'RIGOS_WSL_CARGO_COMPONENT_MISSING=%s\n' "$component" >&2
    missing=1
  fi
done

if [ "$missing" -ne 0 ]; then
  printf '%s\n' 'Install the missing Rust component inside this WSL distribution, then rerun scripts/verify-wsl.ps1.' >&2
  exit 127
fi

printf 'RIGOS_WSL_REPO=%s\n' "$repo"
printf 'RIGOS_WSL_CARGO=%s\n' "$(command -v cargo)"
exec bash ./scripts/verify.sh
'@

$SavedErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    & wsl.exe @WslPrefix -- bash -lc $Shell -- $LinuxRepo 2>&1 |
        ForEach-Object { Write-Host $_ }
    $ExitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $SavedErrorActionPreference
}

if ($ExitCode -ne 0) {
    throw "RIGOS_WSL_SOURCE_GATE_FAILED: exit $ExitCode"
}

Write-Host "RIGOS_WSL_SOURCE_GATE=PASS"
