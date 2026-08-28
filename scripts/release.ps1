<#
.SYNOPSIS
  Cut a release: bump the version in all five version-synchronised files, commit, tag, and push.
.EXAMPLE
  ./scripts/release.ps1 -Version 0.2.0
#>
param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^\d+\.\d+\.\d+$')]
  [string]$Version,

  # CPE-1841: bump the five version-synchronised files and stop -- no git add/commit/tag/push. Exists so the
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
#
# CPE-1852: and that check now runs for ALL manifests before ANY of them is written. It used to fire
# per file, mid-loop, so a Cargo.toml that failed the guard aborted with "Refusing to write" after
# package.json and tauri.conf.json were already at the new version on disk -- a message true of the
# file it named and false of the run, leaving two of CLAUDE.md's five version-synchronised files
# bumped and uncommitted. A dirty tree after a release operation reads as unrelated noise and gets
# committed by accident or discarded along with real work; that is how package-lock.json ended up
# three releases behind. So the bump is split in two: New-ManifestVersionPlan reads, validates and
# computes the replacement text WITHOUT touching disk, and only once every plan exists does the write
# loop run. Nothing is written unless everything can be.

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

# CPE-1853: package-lock.json carries the app version in TWO places -- the root object's "version" and
# `packages[""]`'s "version" (the lockfile's own entry for the root package). Both must move together;
# bumping one and leaving the other is a half-stale lockfile, which is exactly how this file drifts.
# So this locator returns BOTH, and its plan declares -ExpectedCount 2: a change that landed only one
# edit fails the count and writes nothing, rather than "succeeding" into the stale state.
#
# The walk is Find-JsonTopLevelVersionValue's, plus a stack of the enclosing key at each depth, because
# `packages[""].version` cannot be identified by depth alone: `packages` is full of sibling entries of
# the identical shape -- `"node_modules/foo": { "version": "1.2.3" }` sits at the SAME depth 3 and is a
# dependency pin. Only the entry keyed by the empty string is the root package. (Root object = depth 1,
# so `packages` is a key at depth 1, `""` a key at depth 2, and its `version` a key at depth 3.)
#
# Returns every match, same contract as the locators above -- `return $hits`, never `return , $hits`.
function Find-NpmLockVersionValues {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Text)

  $hits = @()
  $keys = @{}   # depth -> the most recent KEY seen at that depth, i.e. the key we descended through
  $depth = 0
  $i = 0
  $n = $Text.Length

  while ($i -lt $n) {
    $ch = $Text[$i]

    if ($ch -eq '"') {
      # Consume a complete string token, honouring backslash escapes, so a `{`, `}` or `"` INSIDE a
      # string can never be mistaken for structure. (package-lock.json is full of "resolved" URLs and
      # base64 "integrity" values.)
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

      # A KEY is a string whose next non-space character is ':'. Anything else is a value and must not
      # touch the key stack -- a dependency whose VALUE is the literal string "version" would otherwise
      # rewrite the enclosing context.
      $j = $i
      while ($j -lt $n -and [char]::IsWhiteSpace($Text[$j])) { $j++ }
      if ($j -ge $n -or $Text[$j] -ne ':') { continue }

      $name = $Text.Substring($tokenStart + 1, $tokenEnd - $tokenStart - 1)
      $keys[$depth] = $name

      $isRootVersion = $depth -eq 1 -and $name -eq 'version'
      $isLockRootPackageVersion = $depth -eq 3 -and $name -eq 'version' -and $keys[1] -eq 'packages' -and $keys[2] -eq ''
      if ($isRootVersion -or $isLockRootPackageVersion) {
        # Each hit is TAGGED with which of the two fields it is, so the caller can insist on one of
        # EACH rather than on a total of two. "Two hits" is also satisfied by two duplicate root-level
        # "version" keys and no `packages` object at all -- measured: count=2, and a total-only guard
        # passes. Unreachable from any npm output (npm never emits duplicate keys, and a real lockfile
        # always has `packages`), so tagging is the guard saying what it means rather than a defect
        # being fixed.
        $kind = if ($isRootVersion) { 'root "version"' } else { 'packages[""]."version"' }

        # ... and the value must be a double-quoted string. `"lockfileVersion": 3` is a number and is
        # not a version of the app in any case; a non-string value here means the file is not the shape
        # this script understands, and dropping the hit makes the count fail loudly.
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
          if ($k -lt $n) { $hits += , @{ Start = $valStart; Length = $k - $valStart; Kind = $kind } }
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

# CPE-1853: src-tauri/Cargo.lock's `[[package]]` entry for the app itself. This is the highest-stakes
# locator in the file: Cargo.lock is ~1000 `[[package]]` blocks and every version in it EXCEPT this one
# is a dependency pin. Rewriting one is precisely the defect CPE-1841 existed to fix, in the file that
# contains the most of them -- and the damage is worse here than in Cargo.toml, because a bad pin in a
# LOCK file is what actually gets resolved and built.
#
# So the scoping is by package identity, not by position: walk each `[[package]]` block (header to the
# next `[`-headed table), and take the `version` only from the block whose `name` is the app's. That
# excludes, by construction:
#   - every other `[[package]]` block, including one whose version happens to equal the app's;
#   - the lockfile's own `version = 3` format marker at the top of the file, which sits before the
#     first `[[package]]` header (and is an unquoted integer besides);
#   - `[[patch.unused]]` / `[metadata]` / any other table shape, even one carrying the app's own NAME
#     -- a `[[patch.unused]]` entry is not the package entry and must not be bumped.
# A rename of the crate makes this find zero and abort the release loudly, which is the intended
# failure: a release script that cannot find the app in its own lockfile must not report a bump.
function Find-CargoLockPackageVersionValue {
  param(
    [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Text,
    [string]$PackageName = 'cross-platform-explorer'
  )

  $hits = @()
  # `\r?$` for the same reason as Find-TomlPackageVersionValue: .NET's multiline `$` matches AFTER the
  # `\r` of a CRLF file, and Cargo.lock is CRLF in a fresh checkout here (core.autocrlf=true) but LF
  # once cargo itself has rewritten it, so both must work.
  foreach ($header in [regex]::Matches($Text, '(?m)^[ \t]*\[\[package\]\][ \t]*\r?$')) {
    $sectionStart = $header.Index + $header.Length
    $next = [regex]::Match($Text.Substring($sectionStart), '(?m)^[ \t]*\[')
    $sectionEnd = if ($next.Success) { $sectionStart + $next.Index } else { $Text.Length }
    $section = $Text.Substring($sectionStart, $sectionEnd - $sectionStart)

    $nameMatch = [regex]::Match($section, '(?m)^[ \t]*name[ \t]*=[ \t]*"([^"]*)"')
    if ($nameMatch.Success -and $nameMatch.Groups[1].Value -eq $PackageName) {
      foreach ($m in [regex]::Matches($section, '(?m)^[ \t]*version[ \t]*=[ \t]*"([^"]*)"')) {
        $g = $m.Groups[1]
        $hits += , @{ Start = $sectionStart + $g.Index; Length = $g.Length }
      }
    }
  }

  return $hits
}

# The sorted, printable signature of a hit set's `Kind` tags, for the -ExpectedKinds guard below. A
# locator that does not tag its hits (all of them except Find-NpmLockVersionValues, which are
# single-hit and need no kinds) yields '(untagged)' -- so asking for kinds from an untagged locator
# fails loudly rather than matching an empty expectation.
function Get-HitKindSignature {
  param([Parameter(Mandatory = $true)][AllowEmptyCollection()][array]$Hits)

  return ((@($Hits | ForEach-Object { if ($null -eq $_.Kind) { '(untagged)' } else { $_.Kind } }) | Sort-Object) -join ' + ')
}

# Phase 1 of the bump: read, validate and compute -- never write. Returns a plan (a hashtable of
# Path / Old / New / Text / Encoding) that the caller hands to Write-ManifestVersionPlan once EVERY
# manifest has produced one. Any failure throws here, with nothing on disk touched by any manifest.
#
# Note the shape of a NON-manifest failure in this phase -- a directory where src-tauri/Cargo.toml
# should be, say. ReadAllBytes below throws a raw .NET exception: atomicity still holds (we are in
# the plan phase, so the tree stays pristine), but the message is the CLR's, with no 'release.ps1:'
# prefix and no statement about tree state. The guarantee is real there; only the wording is not ours.
function New-ManifestVersionPlan {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$NewVersion,
    [Parameter(Mandatory = $true)][string]$Locator,
    [Parameter(Mandatory = $true)][string]$What,
    # CPE-1853: how many values in THIS file are the app's own version. One for the three manifests and
    # for Cargo.lock's `[[package]]` entry; TWO for package-lock.json, which carries it at the root and
    # again in `packages[""]`. Declared per file rather than inferred, so "the locator found fewer than
    # this file is supposed to have" is a loud failure instead of a half-bumped lockfile.
    [int]$ExpectedCount = 1,
    # CPE-1853 (Reviewer): WHICH values, not just how many. A count of 2 on package-lock.json is also
    # satisfied by two duplicate root-level "version" keys and no `packages` object -- measured, and a
    # total-only guard passes it. Naming the kinds makes the guard say what it means. Empty = count only.
    [string[]]$ExpectedKinds = @()
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

  # Spelled as a word for 1 and 2 so the message reads as English ("expected exactly one ...",
  # "expected exactly two ..."); anything else falls back to the numeral rather than inventing words.
  $expectedWord = switch ($ExpectedCount) { 1 { 'one' } 2 { 'two' } default { "$ExpectedCount" } }

  $hits = @(& $Locator $text)
  if ($hits.Count -ne $ExpectedCount) {
    throw ("release.ps1: expected exactly {0} {1} in {2}, found {3}. No manifest was written -- every " -f $expectedWord, $What, $Path, $hits.Count) +
      "manifest is validated before any is written, so the working tree is exactly as it was. A manifest " +
      "that no longer matches must fail the release loudly, not be written back unchanged and reported as bumped."
  }

  # ... and, where the caller named them, ONE OF EACH KIND rather than N of anything.
  $wantKindSignature = ((@($ExpectedKinds) | Sort-Object) -join ' + ')
  if ($ExpectedKinds.Count -gt 0 -and (Get-HitKindSignature -Hits $hits) -ne $wantKindSignature) {
    throw ("release.ps1: expected the {0} in {1} to be exactly [{2}], found [{3}]. No manifest was " -f $What, $Path, $wantKindSignature, (Get-HitKindSignature -Hits $hits)) +
      "written -- the right NUMBER of version values is not the same as the right ONES, and a lockfile " +
      "that no longer has the shape this script understands must fail the release loudly."
  }

  # Splice from the LAST hit backwards: an earlier splice would shift every later offset, and the
  # replacement is not the same length as what it replaces. Old values are read off the ORIGINAL text
  # first, in source order, so the reported `old -> new` is what the file actually said.
  $ordered = @($hits | Sort-Object { $_.Start })
  $olds = @($ordered | ForEach-Object { $text.Substring($_.Start, $_.Length) })
  $updated = $text
  for ($idx = $ordered.Count - 1; $idx -ge 0; $idx--) {
    $hit = $ordered[$idx]
    $updated = $updated.Substring(0, $hit.Start) + $NewVersion + $updated.Substring($hit.Start + $hit.Length)
  }
  # Normally one distinct old value. Two DIFFERENT ones means the file was already internally
  # inconsistent (package-lock.json's root and packages[""] having drifted apart), which the bump
  # repairs -- so report both rather than picking one and implying the other agreed.
  $old = (($olds | Sort-Object -Unique) -join " / ")

  # Post-condition on the exact text about to be written, through the same locator: every value it
  # finds must now BE $NewVersion, and there must still be exactly as many as we were promised.
  # Catches a splice that landed at the wrong offset, a multi-hit splice that updated only some of its
  # hits, and any future locator change that stops agreeing with itself.
  $check = @(& $Locator $updated)
  $spliceFailed = $check.Count -ne $ExpectedCount
  if ($ExpectedKinds.Count -gt 0 -and (Get-HitKindSignature -Hits $check) -ne $wantKindSignature) { $spliceFailed = $true }
  foreach ($c in $check) {
    if ($updated.Substring($c.Start, $c.Length) -ne $NewVersion) { $spliceFailed = $true }
  }
  if ($spliceFailed) {
    throw "release.ps1: splice check failed for $Path -- the $What did not read back as $NewVersion. No manifest was written."
  }

  # A hashtable, not a PSCustomObject, because Write-ManifestVersionPlan's parameter is typed
  # [hashtable]$Plan -- a [pscustomobject] fails argument transformation there ('Cannot process
  # argument transformation on parameter Plan') instead of being written. THAT is what makes the
  # choice load-bearing.
  #
  # It is NOT about array flattening, which an earlier version of this comment claimed. Measured
  # under Windows PowerShell 5.1: `@( pscustomobject; pscustomobject; pscustomobject )` gives
  # Count=3 exactly as the hashtable case does; `@( $null; ht; ht )` gives Count=3 with element[0]
  # simply $null (a null plan does not collapse the array); `@( ht )` gives Count=1, so a future
  # single-manifest variant is safe. A maintainer who checks a false claim and 'corrects' it could
  # remove the real constraint, so the real one is named here.
  #
  # The stray-output hazard IS real: a function emitting two objects makes the caller's array
  # Count=4 and shifts every plan after it. Audited -- every expression in this function is either
  # assigned to a variable or sits inside an if(), so nothing but this return reaches the output
  # stream. Keep it that way.
  return @{ Path = $Path; Old = $old; New = $NewVersion; Text = $updated; Encoding = $writeEncoding; Places = $ordered.Count }
}

# Phase 2: write one already-validated plan. Nothing is decided here -- by the time this runs, every
# manifest in the set has produced a plan, so the only remaining failure is I/O.
function Write-ManifestVersionPlan {
  param([Parameter(Mandatory = $true)][hashtable]$Plan)

  [System.IO.File]::WriteAllText($Plan.Path, $Plan.Text, $Plan.Encoding)
  # The place count is printed when a file carries the version more than once, so package-lock.json's
  # two edits are visible in the release output rather than being taken on trust.
  $places = if ($Plan.Places -gt 1) { " ({0} places)" -f $Plan.Places } else { "" }
  Write-Host ("  {0}: {1} -> {2}{3}" -f $Plan.Path, $Plan.Old, $Plan.New, $places)
}

# CLAUDE.md's five version-synchronised files, all of them, in one list:
#
# 1. package.json              -- the ROOT object's "version"
# 2. src-tauri/tauri.conf.json -- the ROOT object's "version"
# 3. src-tauri/Cargo.toml      -- `version` inside [package]
# 4. package-lock.json         -- the ROOT object's "version" AND packages[""]'s  (TWO places)
# 5. src-tauri/Cargo.lock      -- `version` in the [[package]] entry named cross-platform-explorer
#
# CPE-1852: plan all of them, THEN write all of them. A throw from any New-ManifestVersionPlan call
# happens with the disk untouched, so a failure on a later file can no longer leave the earlier ones
# bumped. Do not collapse this back into a per-file update loop.
#
# CPE-1853: 4 and 5 used to be manual, and they are the two that got missed -- CLAUDE.md records
# package-lock.json sitting three releases behind (0.57.64 vs 0.57.67). Nothing failed when they
# drifted: neither build passes --locked, so both lockfiles are silently rewritten at build time and
# the stale version surfaces only as a dirty working tree, which reads as unrelated noise. Keep this
# list and CLAUDE.md's "keep five files in sync" list identical -- src/lib/releaseVersionBump.test.ts
# cross-checks them and reds if a file is added to one and not the other.
$plans = @(
  New-ManifestVersionPlan -Path (Join-Path $repo "package.json") -NewVersion $Version -Locator 'Find-JsonTopLevelVersionValue' -What 'top-level "version" key'
  New-ManifestVersionPlan -Path (Join-Path $repo "src-tauri/tauri.conf.json") -NewVersion $Version -Locator 'Find-JsonTopLevelVersionValue' -What 'top-level "version" key'
  New-ManifestVersionPlan -Path (Join-Path $repo "src-tauri/Cargo.toml") -NewVersion $Version -Locator 'Find-TomlPackageVersionValue' -What 'version key inside [package]'
  New-ManifestVersionPlan -Path (Join-Path $repo "package-lock.json") -NewVersion $Version -Locator 'Find-NpmLockVersionValues' -What 'app "version" keys' -ExpectedCount 2 -ExpectedKinds 'root "version"', 'packages[""]."version"'
  New-ManifestVersionPlan -Path (Join-Path $repo "src-tauri/Cargo.lock") -NewVersion $Version -Locator 'Find-CargoLockPackageVersionValue' -What 'version key in the [[package]] entry named cross-platform-explorer'
)

$written = @()
foreach ($plan in $plans) {
  try {
    Write-ManifestVersionPlan -Plan $plan
  }
  catch {
    # Everything validated, so the usual cause is I/O -- a read-only file, a lock, a full disk. It is
    # not the ONLY cause: PARAMETER BINDING lands here too (hand Write-ManifestVersionPlan a
    # [pscustomobject] and "Cannot process argument transformation on parameter 'Plan'" arrives
    # through this catch). Only reachable via a future edit, but it is in scope, so the message
    # reports the exception rather than asserting the disk was at fault.
    #
    # Nothing makes three WriteAllText calls atomic, so this is the one path where a partial bump is
    # possible -- and the report has to be true of the run, which is the whole point of CPE-1852.
    if ($written.Count -eq 0) {
      # The FIRST write failed: nothing landed and the tree is clean. Saying "Already written: none.
      # Revert those files" would tell an operator to undo nothing -- the same run-untrue message
      # class this ticket exists to delete, merely inverted. So say what actually happened.
      throw ("release.ps1: failed writing {0}: {1} -- No manifest was written; the working tree is " -f
        $plan.Path, $_.Exception.Message) +
        "unchanged and there is nothing to revert. No commit, tag or push happened."
    }
    throw ("release.ps1: failed writing {0}: {1} -- PARTIAL BUMP. Already written at v{2}: {3}. " -f
      $plan.Path, $_.Exception.Message, $Version, ($written -join ", ")) +
      "Revert those files before retrying (git checkout -- <paths>); no commit, tag or push happened."
  }
  $written += $plan.Path
}

Write-Host "Bumped version to $Version in package.json, tauri.conf.json, Cargo.toml, package-lock.json (2 places), Cargo.lock" -ForegroundColor Green

if ($BumpOnly) {
  Write-Host "-BumpOnly: stopping before git add/commit/tag/push (no release was cut)." -ForegroundColor Yellow
  exit 0
}

# Git writes ordinary progress text to stderr. With $ErrorActionPreference = "Stop",
# PowerShell treats that as a terminating NativeCommandError and aborts the script
# mid-release — leaving the version bumped but uncommitted and untagged.
# So: relax the preference around git, and check $LASTEXITCODE explicitly instead.
$ErrorActionPreference = "Continue"

# CPE-1967 swept every external-tool wrapper in the repo, not only `scripts/*.mjs`, and this is the
# one place it found a real UNTIMED spawn that it deliberately did NOT cap. Recorded here rather than
# only in a PR body, so the next reader knows it was looked at and decided, not missed:
#
#   · Three of the four `git` calls in this file (`add`, `commit`, `tag`) are local and cannot stall
#     on anything but a wedged filesystem. All four go through `Invoke-Git` below, whose single
#     `& git @Args` is the ONLY place this file executes git at all — there are no git reads earlier,
#     which an earlier draft of this very bullet claimed. (It said "four of the five calls", and it
#     contradicted the derivation two bullets down. Miscounted in the block rewritten to stop
#     miscounting — the same shape as this ticket's F1-F4, which is why it is corrected out loud.)
#   · `push` is network-bound and genuinely can hang — the same stalled-transport shape the rest of
#     CPE-1967 is about.
#   · What makes it different: this script is ATTENDED BY CONSTRUCTION. That was the load-bearing
#     claim, so it is DERIVED rather than asserted (CPE-1933). `git grep -i 'release\.ps1'` over the
#     tracked tree returns every reference, and sorting them by kind:
#       — RELEASING.md and `.claude/commands/run.md` — instructions for a HUMAN to type it.
#       — CLAUDE.md — prose describing it.
#       — `.github/workflows/release.yml:139`, `release-sidecar.yml:592` and
#         `.github/workflows/scripts/catalog-version.sh:84` — all three are COMMENT lines that merely
#         cite this file's encoding fix (CPE-1834) or its push behaviour. No `run:` invokes it.
#       — Ticketing/**, docs/** — history and design notes.
#       — `src/lib/releaseVersionBump.test.ts` and `src/lib/appVersionSync.test.ts` — the ONE genuinely
#         unattended caller, and it is why this had to be derived rather than assumed. That harness
#         runs in `ci.yml`'s `frontend` job and EXECUTES the real script — but only ever with
#         `-BumpOnly`, and `-BumpOnly` `exit 0`s at line ~492, above the `$ErrorActionPreference`
#         line below and above `Invoke-Git`'s definition. So no CI path reaches a `git` call in this
#         file at all.
#     Net: the git section runs only for a human at a terminal. There is no 360-minute Actions default
#     underneath it and no silent budget to blow through — the operator IS the timeout, and they can
#     see exactly which command is stuck.
#   · And the cost of getting a cap wrong here is asymmetric. PowerShell has no `timeout` parameter
#     for a native command; bounding one means `Start-Process` + `WaitForExit(ms)` + a kill, which
#     would abort a push mid-transfer on a merely slow connection and leave a tagged-but-unpushed
#     tree. A hang a human can Ctrl-C is a better failure than that.
#
# If this script ever gains an unattended caller that reaches PAST the `-BumpOnly` exit — a scheduled
# task, a workflow `run:`, or a test that drops the switch — this reasoning expires and the push needs
# a bound. `http.lowSpeedLimit`/`http.lowSpeedTime`, git's own stall detector, is the right mechanism
# there; process-killing is not.
function Invoke-Git {
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args)
  & git @Args 2>&1 | ForEach-Object { Write-Host $_ }
  if ($LASTEXITCODE -ne 0) {
    Write-Host "git $($Args -join ' ') failed with exit code $LASTEXITCODE" -ForegroundColor Red
    exit $LASTEXITCODE
  }
}

# CPE-1853: all five, or the release commit ships a version bump with the lockfiles left behind --
# which is the drift this ticket closed, merely moved one step later.
Invoke-Git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml package-lock.json src-tauri/Cargo.lock
Invoke-Git commit -m "release v$Version"
Invoke-Git tag "v$Version"
Invoke-Git push origin HEAD --tags

Write-Host ""
Write-Host "Pushed tag v$Version. GitHub Actions is now building installers." -ForegroundColor Cyan
Write-Host "Watch it with:  gh run watch" -ForegroundColor Yellow
Write-Host "Publish the draft when green:  gh release edit v$Version --draft=false" -ForegroundColor Yellow
