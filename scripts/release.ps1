<#
.SYNOPSIS
  Cut a release: bump the version in all three manifests, commit, tag, and push.
.EXAMPLE
  ./scripts/release.ps1 -Version 0.2.0
#>
param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^\d+\.\d+\.\d+$')]
  [string]$Version
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

Write-Host "Releasing v$Version..." -ForegroundColor Cyan

# Bare Get-Content/Set-Content (no -Encoding) reads and writes the system ANSI
# code page on Windows PowerShell 5.1 (CP1252 here), which is lossy for any
# non-ASCII character in either direction: `Get-Content -Raw` on a BOM-less
# UTF-8 file (all three manifests are BOM-less UTF-8) misdecodes multi-byte
# sequences into mojibake before we ever touch the string, and a bare
# `Set-Content` re-corrupts whatever survived on the way out. `-Encoding utf8`
# fixes the write but adds a UTF-8 BOM instead (its own corruption shape — see
# the mojibake guard's `bom` check). [System.IO.File]::ReadAllText /
# WriteAllText with an explicit BOM-less UTF8Encoding sidesteps every trap on
# both ends and behaves identically on Windows PowerShell 5.1 and PowerShell 7+.
# See CPE-1834.
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

# 1. package.json
$pkgPath = Join-Path $repo "package.json"
$pkg = [System.IO.File]::ReadAllText($pkgPath, $utf8NoBom)
$pkg = $pkg -replace '("version"\s*:\s*")[^"]+(")', "`${1}$Version`$2"
[System.IO.File]::WriteAllText($pkgPath, $pkg, $utf8NoBom)

# 2. src-tauri/tauri.conf.json
$confPath = Join-Path $repo "src-tauri/tauri.conf.json"
$conf = [System.IO.File]::ReadAllText($confPath, $utf8NoBom)
$conf = $conf -replace '("version"\s*:\s*")[^"]+(")', "`${1}$Version`$2"
[System.IO.File]::WriteAllText($confPath, $conf, $utf8NoBom)

# 3. src-tauri/Cargo.toml  (only the first [package] version line)
$cargoPath = Join-Path $repo "src-tauri/Cargo.toml"
$cargo = [System.IO.File]::ReadAllText($cargoPath, $utf8NoBom)
$cargo = $cargo -replace '(?m)^(version\s*=\s*")[^"]+(")', "`${1}$Version`$2"
[System.IO.File]::WriteAllText($cargoPath, $cargo, $utf8NoBom)

Write-Host "Bumped version to $Version in package.json, tauri.conf.json, Cargo.toml" -ForegroundColor Green

# Git writes ordinary progress text to stderr. With $ErrorActionPreference = "Stop",
# PowerShell treats that as a terminating NativeCommandError and aborts the script
# mid-release — leaving the version bumped but uncommitted and untagged.
# So: relax the preference around git, and check $LASTEXITCODE explicitly instead.
$ErrorActionPreference = "Continue"

function Invoke-Git {
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args)
  & git @Args 2>&1 | ForEach-Object { Write-Host $_ }
  if ($LASTEXITCODE -ne 0) {
    Write-Host "git $($Args -join ' ') failed with exit code $LASTEXITCODE" -ForegroundColor Red
    exit $LASTEXITCODE
  }
}

Invoke-Git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml
Invoke-Git commit -m "release v$Version"
Invoke-Git tag "v$Version"
Invoke-Git push origin HEAD --tags

Write-Host ""
Write-Host "Pushed tag v$Version. GitHub Actions is now building installers." -ForegroundColor Cyan
Write-Host "Watch it with:  gh run watch" -ForegroundColor Yellow
Write-Host "Publish the draft when green:  gh release edit v$Version --draft=false" -ForegroundColor Yellow
