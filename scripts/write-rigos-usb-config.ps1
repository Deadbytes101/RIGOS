[CmdletBinding()]
param(
    [string]$DriveLetter,
    [string]$NodeName = "rig01",
    [string]$Timezone = "Asia/Bangkok",
    [string]$PoolHost = "139.99.69.109",
    [ValidateRange(1, 65535)]
    [int]$PoolPort = 10001,
    [switch]$Tls,
    [ValidateRange(1, 1024)]
    [int]$Threads = 2,
    [string]$IdentityAlias = "main-xmr",
    [string]$IdentityValue
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-Slug {
    param([string]$Name, [string]$Value)
    if ($Value -notmatch '^[A-Za-z0-9][A-Za-z0-9-]{0,63}$') {
        throw "$Name is not a valid RIGOS slug"
    }
}

function Write-Utf8NoBom {
    param([string]$Path, [string]$Content)
    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

Assert-Slug -Name "NodeName" -Value $NodeName
Assert-Slug -Name "IdentityAlias" -Value $IdentityAlias

if ($PoolHost.Length -lt 1 -or $PoolHost.Length -gt 253 -or $PoolHost -match '\s') {
    throw "PoolHost is invalid"
}
if ($Timezone.Length -lt 1 -or $Timezone.Length -gt 128 -or $Timezone.StartsWith('/') -or $Timezone.Contains('..')) {
    throw "Timezone is invalid"
}

if ([string]::IsNullOrEmpty($DriveLetter)) {
    $volume = Get-Volume |
        Where-Object FileSystemLabel -eq 'EFI_SYSTEM' |
        Select-Object -First 1
    if (-not $volume) {
        throw "EFI_SYSTEM volume was not found"
    }
    if (-not $volume.DriveLetter) {
        throw "EFI_SYSTEM volume has no drive letter"
    }
    $DriveLetter = [string]$volume.DriveLetter
}
$DriveLetter = $DriveLetter.Trim().TrimEnd(':')
if ($DriveLetter -notmatch '^[A-Za-z]$') {
    throw "DriveLetter must contain one drive letter"
}
$volume = Get-Volume -DriveLetter $DriveLetter
if ($volume.FileSystemLabel -ne 'EFI_SYSTEM') {
    throw "$DriveLetter`: is not the RIGOS EFI_SYSTEM volume"
}

if ([string]::IsNullOrEmpty($IdentityValue)) {
    $IdentityValue = Read-Host "Paste the public mining identity or wallet address"
}
$unsafeCharacters = @(
    $IdentityValue.ToCharArray() |
        Where-Object { [int]$_ -lt 33 -or [int]$_ -gt 126 }
)
if (
    [string]::IsNullOrEmpty($IdentityValue) -or
    $IdentityValue.Length -gt 512 -or
    $IdentityValue -match '\s' -or
    $unsafeCharacters.Count -ne 0
) {
    throw "IdentityValue must be 1 to 512 visible ASCII characters with no whitespace"
}

$root = "$DriveLetter`:\rigos"
$flightDirectory = Join-Path $root "flight-sheets"
$identityDirectory = Join-Path $root "identities"
New-Item -ItemType Directory -Force $flightDirectory | Out-Null
New-Item -ItemType Directory -Force $identityDirectory | Out-Null

$rigConfig = @"
RIGOS_CONFIG_VERSION=1
NODE_NAME=$NodeName
TIMEZONE=$Timezone
FLIGHT_SOURCE=native
FLIGHT_REF=xmr
MINER_START_MODE=on_boot
"@

$flightSheet = [ordered]@{
    schema = "rigos.flight-sheet/v1"
    name = "xmr"
    coin = "XMR"
    backend = "xmrig"
    algorithm = "rx/0"
    pools = @(
        [ordered]@{
            host = $PoolHost
            port = $PoolPort
            tls = [bool]$Tls
            priority = 0
        }
    )
    identity_ref = $IdentityAlias
    worker_template = "{node_name}"
    cpu = [ordered]@{
        threads = $Threads
        huge_pages = $true
        max_threads_hint = 100
    }
} | ConvertTo-Json -Depth 8

$identitySeed = [ordered]@{
    schema = "rigos.identity-seed/v1"
    alias = $IdentityAlias
    kind = "mining_identity"
    value = $IdentityValue
} | ConvertTo-Json -Depth 4

$rigPath = Join-Path $root "rig.conf"
$flightPath = Join-Path $flightDirectory "xmr.json"
$identityPath = Join-Path $identityDirectory "$IdentityAlias.json"
Write-Utf8NoBom -Path $rigPath -Content ($rigConfig.TrimEnd() + "`n")
Write-Utf8NoBom -Path $flightPath -Content ($flightSheet + "`n")
Write-Utf8NoBom -Path $identityPath -Content ($identitySeed + "`n")

$readBack = Get-Content -Raw -LiteralPath $identityPath | ConvertFrom-Json
if (
    $readBack.schema -ne "rigos.identity-seed/v1" -or
    $readBack.alias -ne $IdentityAlias -or
    $readBack.kind -ne "mining_identity" -or
    $readBack.value -ne $IdentityValue
) {
    throw "Identity seed verification failed after writing"
}

$suffixLength = [Math]::Min(4, $IdentityValue.Length)
$suffix = $IdentityValue.Substring($IdentityValue.Length - $suffixLength)
Write-Host "RIGOS USB provisioning written"
Write-Host "Volume       $DriveLetter`: EFI_SYSTEM"
Write-Host "Node         $NodeName"
Write-Host "Pool         $PoolHost`:$PoolPort"
Write-Host "TLS          $([bool]$Tls)"
Write-Host "Threads      $Threads"
Write-Host "Identity     $IdentityAlias suffix ****$suffix"
Write-Host ""
Get-ChildItem -LiteralPath $root -Recurse -File |
    ForEach-Object {
        $hash = Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256
        [PSCustomObject]@{
            Path = $_.FullName
            Length = $_.Length
            SHA256 = $hash.Hash
        }
    } |
    Format-Table -AutoSize

Write-Host "Eject the USB safely before booting RIGOS"
