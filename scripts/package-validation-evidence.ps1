#requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^v0\.0\.1-rc[1-9][0-9]*-rig[0-9]{2}-[0-9]{8}T[0-9]{6}Z$')][string]$RunId,
    [Parameter(Mandatory)][string]$Raw,
    [Parameter(Mandatory)][string]$Public,
    [Parameter(Mandatory)][string]$PrivateOutput,
    [Parameter(Mandatory)][string]$RecipientFile,
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{2,63}$')][string]$RecipientSetId,
    [Parameter(Mandatory)][ValidatePattern('^v0\.0\.1-rc[1-9][0-9]*$')][string]$ReleaseCandidate,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{40}$')][string]$SourceCommit,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$BinarySha256,
    [Parameter(Mandatory)][ValidatePattern('^rig[0-9]{2}$')][string]$NodeAlias,
    [Parameter(Mandatory)][string]$HardwareClass,
    [Parameter(Mandatory)][ValidateSet(12, 13)][int]$DebianMajor,
    [string]$AgePath = 'age', [string]$ZstdPath = 'zstd', [string]$TarPath = 'tar'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-Tool([string]$Name) {
    $tool = Get-Command -Name $Name -CommandType Application -ErrorAction Stop
    return $tool.Source
}
function Invoke-Native([string]$File, [string[]]$Arguments) {
    $info=[Diagnostics.ProcessStartInfo]::new(); $info.FileName=$File; $info.UseShellExecute=$false
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
    foreach($argument in $Arguments){[void]$info.ArgumentList.Add($argument)}
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$info
    if(-not $process.Start()){throw "Failed to start native command: $File"}
    $stdoutTask=$process.StandardOutput.ReadToEndAsync(); $stderrTask=$process.StandardError.ReadToEndAsync()
    if(-not $process.WaitForExit(120000)){try{$process.Kill($true)}catch{};throw "Native command timed out: $File"}
    $stdout=$stdoutTask.GetAwaiter().GetResult(); $stderr=$stderrTask.GetAwaiter().GetResult()
    if($stderr.Length -gt 65536){$stderr=$stderr.Substring(0,65536)}
    if($process.ExitCode -ne 0){throw "Native command failed ($($process.ExitCode)): $File`n$stderr"}
    return $stdout
}
function Get-Sha256([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$rawPath = (Resolve-Path -LiteralPath $Raw).Path
if ($rawPath.StartsWith($repo + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Raw evidence must remain outside the Git repository.'
}
$recipientPath = (Resolve-Path -LiteralPath $RecipientFile).Path
$age = Resolve-Tool $AgePath; $zstd = Resolve-Tool $ZstdPath; $tar = Resolve-Tool $TarPath
if (-not $RunId.StartsWith($ReleaseCandidate + '-', [StringComparison]::Ordinal)) { throw 'Run ID and release candidate mismatch.' }
$requiredRaw = @(
    'raw-meta/rigosd.sha256','raw-meta/raw-manifest.json','raw-meta/RAW-SHA256SUMS','raw-meta/result-input.json','raw-meta/privacy.json',
    'inventory/os-release.txt','inventory/uname.txt','inventory/lscpu.txt','inventory/runtime-libraries.txt',
    'inspection/machine-inspect.json','inspection/miner-stopped.json','inspection/miner-running-no-api.json',
    'inspection/miner-running-loopback-api.json','inspection/doctor.json','mutation/before.sha256','mutation/after.sha256','mutation/comparison.txt',
    'verification/probe-timeout.json','verification/probe-processes-after.txt'
)
foreach($relative in $requiredRaw){if(-not(Test-Path -LiteralPath (Join-Path $rawPath $relative))){throw "Missing required raw evidence: $relative"}}
$rigosHashLine=Get-Content -Raw -LiteralPath (Join-Path $rawPath 'raw-meta/rigosd.sha256')
if($rigosHashLine -notmatch [regex]::Escape($BinarySha256)){throw 'Collected binary does not match the authoritative SHA-256.'}
$manifestPath = Join-Path $repo 'Cargo.toml'
$recipientJson = & cargo run --manifest-path $manifestPath --quiet --locked -p rigos-evidence -- recipients --file $recipientPath
if ($LASTEXITCODE -ne 0) { throw 'Recipient validation failed.' }
$recipientSet = ($recipientJson -join "`n") | ConvertFrom-Json

$publicParent = Split-Path -Parent $Public
New-Item -ItemType Directory -Force -Path $publicParent, $PrivateOutput | Out-Null
$publicPath = [IO.Path]::GetFullPath($Public)
if (Test-Path -LiteralPath $publicPath) { throw "Public run directory already exists: $publicPath" }
New-Item -ItemType Directory -Path $publicPath | Out-Null
try {
    $redactionJson = & cargo run --manifest-path $manifestPath --quiet --locked -p rigos-evidence -- sanitize --raw $rawPath --public $publicPath --node-alias $NodeAlias
    if ($LASTEXITCODE -ne 0) { throw 'Evidence sanitization failed.' }
    ($redactionJson -join "`n") | Set-Content -LiteralPath (Join-Path $publicPath 'redaction-report.json') -Encoding utf8NoBOM

    $resultPath = Join-Path $rawPath 'raw-meta/result-input.json'
    if (-not (Test-Path -LiteralPath $resultPath)) { throw 'Missing raw-meta/result-input.json.' }
    $result = Get-Content -Raw -LiteralPath $resultPath | ConvertFrom-Json
    if ($result.run_id -ne $RunId) { throw 'Result run ID mismatch.' }
    $allowedChecks=@('binary.sha256_matches_authoritative_rc','runtime.no_illegal_instruction','machine.real_hwmon_observed','machine.huge_pages_observed','miner.stopped_snapshot_valid','miner.running_without_api_snapshot_valid','miner.loopback_api_snapshot_valid','inspection.zero_persistent_mutation','output.no_secret_leak')
    $publicChecks=@()
    foreach ($check in $result.checks) {
        if ($check.result -notin @('pass','fail','not_applicable','blocked')) { throw "Invalid result: $($check.result)" }
        if($check.id -notin $allowedChecks){throw "Unknown validation check: $($check.id)"}
        $publicChecks += [ordered]@{id=[string]$check.id;result=[string]$check.result}
    }
    if($publicChecks.Count -ne $allowedChecks.Count){throw 'Validation result does not contain every mandatory check.'}
    $publicResult=[ordered]@{schema='rigos.physical-validation-result/v1';run_id=$RunId;overall=[string]$result.overall;checks=$publicChecks}
    $publicResult | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $publicPath 'result.json') -Encoding utf8NoBOM

    $contractTemp=Join-Path $env:TEMP ("rigos-contract-"+[guid]::NewGuid().ToString('N')); New-Item -ItemType Directory -Path $contractTemp | Out-Null
    try {
        Copy-Item (Join-Path $publicPath 'inspection/machine-inspect.json') (Join-Path $contractTemp 'machine.json')
        Copy-Item (Join-Path $publicPath 'inspection/miner-running-loopback-api.json') (Join-Path $contractTemp 'miner.json')
        Copy-Item (Join-Path $publicPath 'inspection/doctor.json') (Join-Path $contractTemp 'doctor.json')
        & cargo run --manifest-path $manifestPath --quiet --locked -p rigos-schema --bin validate-cli-output -- $contractTemp
        if($LASTEXITCODE -ne 0){throw 'Public inspection schema validation failed.'}
    } finally {Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $contractTemp}

    $partial = Join-Path $PrivateOutput "$RunId.tar.zst.age.partial"
    $archive = Join-Path $PrivateOutput "$RunId.tar.zst.age"
    if ((Test-Path $partial) -or (Test-Path $archive)) { throw 'Private archive output already exists.' }
    $tarTemp = Join-Path $env:TEMP "$RunId.tar"
    $zstdTemp = "$tarTemp.zst"
    try {
        Invoke-Native $tar @('-cf', $tarTemp, '-C', $rawPath, '.')
        Invoke-Native $zstd @('-q', '-19', '--no-progress', '-f', $tarTemp, '-o', $zstdTemp)
        $ageArgs = @('-o', $partial)
        foreach ($recipient in $recipientSet.recipients) { $ageArgs += @('-r', [string]$recipient) }
        $ageArgs += $zstdTemp
        Invoke-Native $age $ageArgs
        if ((Get-Item -LiteralPath $partial).Length -le 0) { throw 'Encrypted archive is empty.' }
        Move-Item -LiteralPath $partial -Destination $archive
    } finally {
        Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $tarTemp, $zstdTemp
        if (Test-Path -LiteralPath $partial) { Remove-Item -Force -LiteralPath $partial }
    }

    $cipher = Get-Item -LiteralPath $archive
    $uname = Get-Content -Raw -LiteralPath (Join-Path $publicPath 'inventory/uname.txt')
    $kernel = ($uname -split "`r?`n")[0]
    $publicHashes = [ordered]@{}
    Get-ChildItem -File -Recurse -LiteralPath $publicPath | Sort-Object FullName | ForEach-Object {
        $relative = [IO.Path]::GetRelativePath($publicPath, $_.FullName).Replace('\','/')
        $publicHashes[$relative] = Get-Sha256 $_.FullName
    }
    $manifest = [ordered]@{
        schema='rigos.physical-validation-manifest/v1'; run_id=$RunId; release_candidate=$ReleaseCandidate
        source_commit=$SourceCommit
        authoritative_binary=[ordered]@{name='rigosd';sha256=$BinarySha256;target='x86_64-unknown-linux-gnu'}
        node=[ordered]@{alias=$NodeAlias;hardware_class=$HardwareClass;architecture='x86_64'}
        runtime=[ordered]@{distribution='Debian GNU/Linux';distribution_major=$DebianMajor;kernel=$kernel}
        started_at=[DateTime]::ParseExact($RunId.Substring($RunId.Length-16,16),'yyyyMMddTHHmmssZ',[Globalization.CultureInfo]::InvariantCulture,[Globalization.DateTimeStyles]::AssumeUniversal).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'); completed_at=[DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
        result=if($result.overall -eq 'pass'){'blocked'}else{$result.overall}; public_evidence_sha256=$publicHashes
        private_archive=[ordered]@{retained=$true;format='tar.zst.age';encryption_schema='rigos.private-archive-encryption/age-x25519-v1'
            recipient_set_id=$RecipientSetId;recipient_set_sha256=$recipientSet.recipient_set_sha256;recipient_count=$recipientSet.recipients.Count
            ciphertext_sha256=(Get-Sha256 $archive);ciphertext_size_bytes=$cipher.Length;decryptability_verified=$false;location_disclosed=$false}
        redaction_policy='rigos.validation-redaction/v1'
    }
    $manifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $publicPath 'manifest.json') -Encoding utf8NoBOM
    $forbiddenPatterns=@(('SENTINEL_'+'SECRET_VALUE'),'Authorization:',('AGE-'+'SECRET-KEY-'),'-----BEGIN PRIVATE KEY-----')
    $forbidden=Get-ChildItem -File -Recurse -LiteralPath $publicPath | Select-String -SimpleMatch -Pattern $forbiddenPatterns
    if($forbidden){throw 'Forbidden secret pattern remains in public evidence.'}
    Get-ChildItem -File -Recurse -LiteralPath $publicPath | Sort-Object FullName | ForEach-Object {
        "$(Get-Sha256 $_.FullName)  $([IO.Path]::GetRelativePath($publicPath, $_.FullName).Replace('\','/'))"
    } | Set-Content -LiteralPath (Join-Path $publicPath 'SHA256SUMS') -Encoding ascii
} catch {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $publicPath
    throw
}
