#requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Archive,
    [Parameter(Mandatory)][string]$Identity,
    [Parameter(Mandatory)][string]$RunId,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$BinarySha256,
    [Parameter(Mandatory)][string]$PublicManifest,
    [string]$AgePath='age', [string]$ZstdPath='zstd', [string]$TarPath='tar'
)
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
function Tool([string]$name) { (Get-Command $name -CommandType Application -ErrorAction Stop).Source }
function Run([string]$file,[string[]]$arguments) {
    $info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=$file;$info.UseShellExecute=$false;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
    foreach($argument in $arguments){[void]$info.ArgumentList.Add($argument)}
    $process=[Diagnostics.Process]::new();$process.StartInfo=$info;if(-not $process.Start()){throw "Failed to start: $file"}
    $outTask=$process.StandardOutput.ReadToEndAsync();$errTask=$process.StandardError.ReadToEndAsync()
    if(-not $process.WaitForExit(120000)){try{$process.Kill($true)}catch{};throw "Native command timed out: $file"}
    $stdout=$outTask.GetAwaiter().GetResult();$stderr=$errTask.GetAwaiter().GetResult();if($stderr.Length -gt 65536){$stderr=$stderr.Substring(0,65536)}
    if($process.ExitCode -ne 0){throw "Native command failed ($($process.ExitCode)): $file`n$stderr"};return $stdout
}
$archivePath=(Resolve-Path -LiteralPath $Archive).Path; $identityPath=(Resolve-Path -LiteralPath $Identity).Path
$manifestPath=(Resolve-Path -LiteralPath $PublicManifest).Path; $manifest=Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if($manifest.run_id -ne $RunId -or $manifest.authoritative_binary.sha256 -ne $BinarySha256){throw 'Public manifest provenance mismatch.'}
$cipherHash=(Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
if($cipherHash -ne $manifest.private_archive.ciphertext_sha256){throw 'Ciphertext hash does not match public manifest.'}
if((Get-Item -LiteralPath $archivePath).Length -ne $manifest.private_archive.ciphertext_size_bytes){throw 'Ciphertext size does not match public manifest.'}
$temp=Join-Path ([IO.Path]::GetTempPath()) ("rigos-verify-"+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    $compressed=Join-Path $temp 'raw.tar.zst'; $tarFile=Join-Path $temp 'raw.tar'; $extract=Join-Path $temp 'raw'
    Run (Tool $AgePath) @('-d','-i',$identityPath,'-o',$compressed,$archivePath)
    Run (Tool $ZstdPath) @('-q','-d','-f',$compressed,'-o',$tarFile)
    New-Item -ItemType Directory -Path $extract | Out-Null; Run (Tool $TarPath) @('-xf',$tarFile,'-C',$extract)
    $rawManifest=Get-Content -Raw -LiteralPath (Join-Path $extract 'raw-meta/raw-manifest.json') | ConvertFrom-Json
    if($rawManifest.run_id -ne $RunId){throw 'Archive run ID mismatch.'}
    $hashLine=Get-Content -Raw -LiteralPath (Join-Path $extract 'raw-meta/rigosd.sha256')
    if($hashLine -notmatch [regex]::Escape($BinarySha256)){throw 'Authoritative binary hash mismatch.'}
    foreach($line in Get-Content -LiteralPath (Join-Path $extract 'raw-meta/RAW-SHA256SUMS')) {
        if($line -notmatch '^([0-9a-f]{64})  (.+)$'){throw 'Malformed inner checksum file.'}
        $path=Join-Path $extract $Matches[2]; if(-not(Test-Path -LiteralPath $path)){throw "Missing archive entry: $($Matches[2])"}
        if((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Matches[1]){throw "Checksum mismatch: $($Matches[2])"}
    }
    $resultPath=Join-Path (Split-Path -Parent $manifestPath) 'result.json'; $result=Get-Content -Raw -LiteralPath $resultPath | ConvertFrom-Json
    $manifest.private_archive.decryptability_verified=$true; if($result.overall -eq 'pass'){$manifest.result='pass'}
    $manifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
    $publicRoot=Split-Path -Parent $manifestPath
    Get-ChildItem -File -Recurse -LiteralPath $publicRoot | Where-Object Name -ne 'SHA256SUMS' | Sort-Object FullName | ForEach-Object {
        "$((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $([IO.Path]::GetRelativePath($publicRoot,$_.FullName).Replace('\','/'))"
    } | Set-Content -LiteralPath (Join-Path $publicRoot 'SHA256SUMS') -Encoding ascii
    Write-Output 'Private archive verification passed.'
} finally { Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $temp }
