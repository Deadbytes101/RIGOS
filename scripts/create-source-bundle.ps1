#requires -Version 7.4
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Output)
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $repo
try {
    if(git status --porcelain){throw 'Source bundle requires a clean working tree.'}
    $commit=(git rev-parse --verify HEAD).Trim(); if($LASTEXITCODE -ne 0){throw 'Cannot resolve HEAD.'}
    $target=[IO.Path]::GetFullPath($Output); $parent=Split-Path -Parent $target
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    if(Test-Path -LiteralPath $target){throw "Output already exists: $target"}
    git bundle create $target HEAD; if($LASTEXITCODE -ne 0){throw 'Git bundle creation failed.'}
    $hash=(Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($target))" | Set-Content -LiteralPath "$target.sha256" -Encoding ascii
    [ordered]@{commit=$commit;bundle=$target;sha256=$hash} | ConvertTo-Json
} finally {Pop-Location}

