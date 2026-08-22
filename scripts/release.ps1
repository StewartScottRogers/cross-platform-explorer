<#
.SYNOPSIS
  Cut a release: bump the version in all three manifests, commit, tag, and push.
.EXAMPLE
  ./scripts/release.ps1 -Version 0.2.0
#>
param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^\d+\.\d+\.\d+$')]
  [string]$Version,

  # CPE-1841: bump the three manifests and stop -- no git add/commit/tag/push. Exists so the
  # version-scoping tests (src/lib/releaseVersionBump.test.ts) can exercise THIS script against
  # throwaway manifest copies in a scratch tree, rather than a re-implementation of its regexes;
  # and so a human can dry-run the bump and read the diff before anything is committed.
  [switch]$BumpOnly
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

# CPE-1841: these three bumps used to be un-anchored `-replace` calls over the whole file --
# '("version"\s*:\s*")[^"]+(")' for the two JSON manifests and '(?m)^(version\s*=\s*")[^"]+(")'
# for Cargo.toml. Neither was scoped to the key it meant, so EVERY version-shaped value in the
# file was rewritten to the app version. Measured against the pre-fix script: a
# `"someTool": { "version": "3.2.1" }` in package.json, a `"wix": { "version": "3.11.2" }` in
# tauri.conf.json, and a long-form `[dependencies.somepkg]` / `version = "1.2.3"` pin in
# Cargo.toml were all silently rewritten to the release version. Dormant only because today's real
# manifests happen to contain no such decoy -- and the long-form dependency table is the ordinary
# way to express a dependency with features, so the trap is one edit away, on the least-exercised
# code path in the repo.
#
# The fix locates the ONE value each bump actually means and splices it in place:
#   - package.json / tauri.conf.json: the "version" key of the ROOT object only, found by walking
#     the text while tracking JSON nesting depth and string escapes. An indentation rule or a
#     `(?m)^` anchor would still match a pretty-printed nested key, and ConvertFrom-Json /
#     ConvertTo-Json would reformat (and reorder) the whole file.
#   - Cargo.toml: a `version = "..."` line inside the `[package]` table only -- i.e. between the
#     `[package]` header and the next `[`-headed table, which is what puts `[dependencies.somepkg]`
#     out of reach.
# Splicing the located value (rather than rebuilding the file) is also what keeps the diff at one
# line with CRLF and the trailing newline byte-preserved: every other byte is carried through
# untouched by construction.
#
# Each locator must find EXACTLY ONE match, in the file as read AND in the text about to be
# written, or the script throws before anything reaches disk. The old code could not fail this way:
# a manifest that stopped matching was written back unchanged and reported as bumped. A release
# that reports success having bumped nothing is the failure shape this repo keeps closing.

# Offsets (Start/Length) of the ROOT object's "version" value, exclusive of its quotes. Returns
# every match found, so the caller can insist on exactly one rather than silently taking the first.
# `return $hits` (no unary comma): PowerShell unrolls the array into the pipeline so the caller's
# `@(...)` sees N elements. `return , $hits` would hand the caller a ONE-element array wrapping the
# whole list -- verified on both Windows PowerShell 5.1 and PowerShell 7 -- which reads as "exactly
# one hit" no matter how many there really are, and then splices at the wrong offset.
function Find-JsonTopLevelVersionValue {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Text)

  $hits = @()
  $depth = 0
  $i = 0
  $n = $Text.Length

  while ($i -lt $n) {
    $ch = $Text[$i]

    if ($ch -eq '"') {
      # Consume a complete string token, honouring backslash escapes, so a `{`, `}` or `"` INSIDE a
      # string can never be mistaken for structure.
      $tokenStart = $i
      $i++
      while ($i -lt $n) {
        if ($Text[$i] -eq '\') { $i += 2; continue }
        if ($Text[$i] -eq '"') { break }
        $i++
      }
      if ($i -ge $n) { break }   # unterminated string: malformed JSON, let the caller's count fail
      $tokenEnd = $i
      $i++

      # Depth 1 == a member of the root object; a nested object's members sit at depth 2 or deeper.
      if ($depth -eq 1 -and $Text.Substring($tokenStart + 1, $tokenEnd - $tokenStart - 1) -eq 'version') {
        # ... and it must be a KEY (next non-space character is ':'), not a value that happens to
        # read "version", and its own value must be a double-quoted string.
        $j = $i
        while ($j -lt $n -and [char]::IsWhiteSpace($Text[$j])) { $j++ }
        if ($j -lt $n -and $Text[$j] -eq ':') {
          $j++
          while ($j -lt $n -and [char]::IsWhiteSpace($Text[$j])) { $j++ }
          if ($j -lt $n -and $Text[$j] -eq '"') {
            $valStart = $j + 1
            $k = $valStart
            while ($k -lt $n) {
              if ($Text[$k] -eq '\') { $k += 2; continue }
              if ($Text[$k] -eq '"') { break }
              $k++
            }
            if ($k -lt $n) { $hits += , @{ Start = $valStart; Length = $k - $valStart } }
          }
        }
      }
      continue
    }

    if ($ch -eq '{' -or $ch -eq '[') { $depth++ }
    elseif ($ch -eq '}' -or $ch -eq ']') { $depth-- }
    $i++
  }

  return $hits
}

# Offsets (Start/Length) of `version = "..."` values inside the `[package]` table only, exclusive of
# the quotes. Same contract as above: every match, so the caller can insist on exactly one.
function Find-TomlPackageVersionValue {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Text)

  $hits = @()
  # `\r?$` because .NET's multiline `$` matches immediately before the `\n`, i.e. AFTER the `\r` of
  # a CRLF file -- and these manifests are CRLF in the working tree (core.autocrlf=true).
  $header = [regex]::Match($Text, '(?m)^[ \t]*\[package\][ \t]*\r?$')
  if (-not $header.Success) { return $hits }   # no [package] table at all: zero hits, caller throws

  $sectionStart = $header.Index + $header.Length
  $next = [regex]::Match($Text.Substring($sectionStart), '(?m)^[ \t]*\[')
  $sectionEnd = if ($next.Success) { $sectionStart + $next.Index } else { $Text.Length }
  $section = $Text.Substring($sectionStart, $sectionEnd - $sectionStart)

  foreach ($m in [regex]::Matches($section, '(?m)^[ \t]*version[ \t]*=[ \t]*"([^"]*)"')) {
    $g = $m.Groups[1]
    $hits += , @{ Start = $sectionStart + $g.Index; Length = $g.Length }
  }

  return $hits
}

function Update-ManifestVersion {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$NewVersion,
    [Parameter(Mandatory = $true)][string]$Locator,
    [Parameter(Mandatory = $true)][string]$What
  )

  if (-not (Test-Path -LiteralPath $Path)) {
    throw "release.ps1: $Path does not exist -- refusing to report a version bump that did not happen."
  }

  # CPE-1841: `[System.IO.File]::ReadAllText($Path, $encoding)` does NOT honour the encoding argument
  # for BOM purposes -- its underlying StreamReader auto-detects and STRIPS a leading BOM even when an
  # explicit BOM-less UTF8Encoding is passed. Measured directly on both Windows PowerShell 5.1 and
  # PowerShell 7.6: reading a file whose bytes start EF BB BF returns a string whose first character is
  # U+007B ('{'), not U+FEFF. So writing back with $utf8NoBom would silently DELETE the BOM -- a second
  # changed line in what is supposed to be a one-line version bump, and an encoding change nobody asked
  # this script to make. Detect it from the raw bytes and write back whatever the file already had; if a
  # manifest's BOM should go, that is the mojibake guard's business (src/lib/mojibakeGuard.test.ts), not
  # a side effect of cutting a release.
  #
  # This is defence in depth, not a live case: none of the three manifests carries a BOM today, and all
  # three sit inside the guard's scanned set. But the guard EXCLUDES Ticketing/ and samples/, where 12
  # tracked files do carry one (CPE-1784), so "no repo file has a BOM" is not the repo-wide invariant it
  # reads as -- it holds for these three files by where the exclusion boundaries happen to fall.
  $rawBytes = [System.IO.File]::ReadAllBytes($Path)
  $hadBom = $rawBytes.Length -ge 3 -and $rawBytes[0] -eq 0xEF -and $rawBytes[1] -eq 0xBB -and $rawBytes[2] -eq 0xBF
  $writeEncoding = if ($hadBom) { New-Object System.Text.UTF8Encoding($true) } else { $utf8NoBom }

  $text = [System.IO.File]::ReadAllText($Path, $utf8NoBom)

  $hits = @(& $Locator $text)
  if ($hits.Count -ne 1) {
    throw ("release.ps1: expected exactly one {0} in {1}, found {2}. Refusing to write -- a manifest that " -f $What, $Path, $hits.Count) +
      "no longer matches must fail the release loudly, not be written back unchanged and reported as bumped."
  }

  $hit = $hits[0]
  $old = $text.Substring($hit.Start, $hit.Length)
  $updated = $text.Substring(0, $hit.Start) + $NewVersion + $text.Substring($hit.Start + $hit.Length)

  # Post-condition on the exact text about to be written, through the same locator: the value it
  # finds must now BE $NewVersion. Catches a splice that landed at the wrong offset, and any future
  # locator change that stops agreeing with itself.
  $check = @(& $Locator $updated)
  if ($check.Count -ne 1 -or $updated.Substring($check[0].Start, $check[0].Length) -ne $NewVersion) {
    throw "release.ps1: post-write check failed for $Path -- the $What did not read back as $NewVersion. Nothing was written."
  }

  [System.IO.File]::WriteAllText($Path, $updated, $writeEncoding)
  Write-Host ("  {0}: {1} -> {2}" -f $Path, $old, $NewVersion)
}

# 1. package.json             -- the ROOT object's "version"
# 2. src-tauri/tauri.conf.json -- the ROOT object's "version"
# 3. src-tauri/Cargo.toml     -- `version` inside [package]
Update-ManifestVersion -Path (Join-Path $repo "package.json") -NewVersion $Version -Locator 'Find-JsonTopLevelVersionValue' -What 'top-level "version" key'
Update-ManifestVersion -Path (Join-Path $repo "src-tauri/tauri.conf.json") -NewVersion $Version -Locator 'Find-JsonTopLevelVersionValue' -What 'top-level "version" key'
Update-ManifestVersion -Path (Join-Path $repo "src-tauri/Cargo.toml") -NewVersion $Version -Locator 'Find-TomlPackageVersionValue' -What 'version key inside [package]'

Write-Host "Bumped version to $Version in package.json, tauri.conf.json, Cargo.toml" -ForegroundColor Green

if ($BumpOnly) {
  Write-Host "-BumpOnly: stopping before git add/commit/tag/push (no release was cut)." -ForegroundColor Yellow
  exit 0
}

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
