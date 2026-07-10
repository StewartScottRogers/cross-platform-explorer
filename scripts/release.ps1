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

# 1. package.json
$pkgPath = Join-Path $repo "package.json"
$pkg = Get-Content $pkgPath -Raw
$pkg = $pkg -replace '("version"\s*:\s*")[^"]+(")', "`${1}$Version`$2"
Set-Content $pkgPath $pkg -NoNewline

# 2. src-tauri/tauri.conf.json
$confPath = Join-Path $repo "src-tauri/tauri.conf.json"
$conf = Get-Content $confPath -Raw
$conf = $conf -replace '("version"\s*:\s*")[^"]+(")', "`${1}$Version`$2"
Set-Content $confPath $conf -NoNewline

# 3. src-tauri/Cargo.toml  (only the first [package] version line)
$cargoPath = Join-Path $repo "src-tauri/Cargo.toml"
$cargo = Get-Content $cargoPath -Raw
$cargo = $cargo -replace '(?m)^(version\s*=\s*")[^"]+(")', "`${1}$Version`$2"
Set-Content $cargoPath $cargo -NoNewline

Write-Host "Bumped version to $Version in package.json, tauri.conf.json, Cargo.toml" -ForegroundColor Green

# Commit, tag, push
git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml
git commit -m "release v$Version"
git tag "v$Version"
git push origin HEAD --tags

Write-Host ""
Write-Host "Pushed tag v$Version. GitHub Actions is now building installers." -ForegroundColor Cyan
Write-Host "Watch it with:  gh run watch" -ForegroundColor Yellow
Write-Host "Publish the draft when green:  gh release edit v$Version --draft=false" -ForegroundColor Yellow
