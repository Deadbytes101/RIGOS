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
$RepositoryForWsl = $Repository.Replace([char]92, [char]47)

$SavedErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $LinuxRepoOutput = & wsl.exe @WslPrefix -- wslpath -a -- $RepositoryForWsl 2>&1
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

$LinuxEntrypoint = "$LinuxRepo/scripts/verify-wsl-entrypoint.sh"

$SavedErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    & wsl.exe @WslPrefix -- bash $LinuxEntrypoint $LinuxRepo 2>&1 |
        ForEach-Object { Write-Host $_ }
    $ExitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $SavedErrorActionPreference
}

if ($ExitCode -ne 0) {
    throw "RIGOS_WSL_SOURCE_GATE_FAILED: exit $ExitCode"
}

Write-Host "RIGOS_WSL_SOURCE_GATE=PASS"
