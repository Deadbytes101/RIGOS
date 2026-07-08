[CmdletBinding()]
param(
    [string]$Repository = (Split-Path -Parent $PSScriptRoot),
    [string]$Distribution = $env:RIGOS_WSL_DISTRO
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

$WslPrefix = @()
if (-not [string]::IsNullOrWhiteSpace($Distribution)) {
    $WslPrefix += @("-d", $Distribution)
}

$Repository = (Resolve-Path -LiteralPath $Repository).Path
$LinuxRepoOutput = & wsl.exe @WslPrefix -- wslpath -a $Repository 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "WSL_PATH_CONVERSION_FAILED: $LinuxRepoOutput"
}

$LinuxRepo = ($LinuxRepoOutput | Select-Object -Last 1).Trim()
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
for tool in cargo rustc python3 bash rg; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'RIGOS_WSL_TOOL_MISSING=%s\n' "$tool" >&2
    missing=1
  fi
done

if [ "$missing" -ne 0 ]; then
  printf '%s\n' 'Install the missing tool inside this WSL distribution, then rerun scripts/verify-wsl.ps1.' >&2
  exit 127
fi

printf 'RIGOS_WSL_REPO=%s\n' "$repo"
printf 'RIGOS_WSL_CARGO=%s\n' "$(command -v cargo)"
exec bash ./scripts/verify.sh
'@

& wsl.exe @WslPrefix -- bash -lc $Shell -- $LinuxRepo
$ExitCode = $LASTEXITCODE
if ($ExitCode -ne 0) {
    throw "RIGOS_WSL_SOURCE_GATE_FAILED: exit $ExitCode"
}

Write-Host "RIGOS_WSL_SOURCE_GATE=PASS"
