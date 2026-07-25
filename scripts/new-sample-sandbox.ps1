<#
.SYNOPSIS
  Copy the pristine samples/ tree into a fresh git-ignored .sandbox/<timestamp>/ directory (CPE-1042),
  so you can edit copies (e.g. save tag changes in the Metadata Studio) without touching the baseline.
.EXAMPLE
  scripts\new-sample-sandbox.ps1
#>
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$src = Join-Path $repo "samples"
if (-not (Test-Path $src)) { throw "samples/ not found at $src" }

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$dest = Join-Path $repo ".sandbox\$stamp"
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Copy-Item -Path (Join-Path $src "*") -Destination $dest -Recurse -Force
# Don't copy the README's "pristine" rule into the sandbox — this copy is meant to be edited.
Remove-Item -Path (Join-Path $dest "README.md") -ErrorAction SilentlyContinue

Write-Output "Sandbox ready (edit freely — it's git-ignored):"
Write-Output "  $dest"
