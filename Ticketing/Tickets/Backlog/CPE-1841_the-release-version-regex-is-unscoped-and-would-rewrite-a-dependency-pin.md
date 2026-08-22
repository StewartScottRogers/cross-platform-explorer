---
id: CPE-1841
title: the release version regex is unscoped, so it would rewrite a dependency pin to the app version
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-21
closed:
---

## Problem

`scripts/release.ps1` bumps the version in three manifests with an un-anchored, unlimited `-replace`.
It is not scoped to the top-level key or the `[package]` block, so **any** version-shaped string in the
file is rewritten to the new app version.

Measured during the CPE-1834 UAT, on both `main` and that PR's branch — this is pre-existing and neither
introduced nor fixed there:

- A `Cargo.toml` carrying a long-form dependency table:
  ```
  [dependencies.somepkg]
  version = "1.2.3"
  ```
  → the dependency's `1.2.3` was rewritten to `9.9.9`, the app version.
- A `package.json` carrying a nested `"someTool": { "version": "3.2.1" }` → rewritten to `9.9.9`.

## Why it matters, and why it is not urgent

**Dormant today.** The real `src-tauri/Cargo.toml` uses only inline `{ version = "…" }` dependency
syntax, and the real `package.json` has a single top-level `"version"` key. Nothing currently trips it.

**But it is a trap on the release path.** The moment anyone adds a long-form dependency table — the
ordinary way to express a dependency with features — a release would silently rewrite that pin to the
app's version number. The build would then either fail confusingly or, worse, resolve a different
dependency version than intended. And the release script is the least-exercised code in the repo,
run when nobody is watching.

It also interacts with the five-files-in-sync rule that already gets missed: a bump that changes more
than the version line makes a dirty tree read as expected noise.

## Acceptance criteria

- [x] Each replacement is scoped to the key it means: `package.json`'s **top-level** `"version"`,
      `tauri.conf.json`'s **top-level** `"version"`, and `Cargo.toml`'s `version` **inside `[package]`**
      only.
- [x] A dependency pin, a nested tool version, and a version-shaped string in a description or URL are
      all left alone. Test each.
- [x] Red-proof: craft a manifest containing both the real version and a decoy, run the bump, assert only
      the real one changed. Then revert the scoping and confirm the test reds.
- [x] Still a one-line diff on the real manifests — CPE-1834's UAT measured `1 1` per file with CRLF and
      the trailing newline preserved, and that must not regress.
- [x] If a manifest ever fails to match at all, the script must fail loudly rather than silently writing
      an unchanged file — check whether it does today and fix it if not. A release that reports success
      having bumped nothing is the same "fails by succeeding" shape this repo keeps closing.

## Notes

Found by the independent UAT during CPE-1834, which was an encoding-only ticket and correctly did not
absorb this. That PR fixed a genuinely subtle adjacent bug: the read side was lossy as well as the write
side, and the two cancelled out, so a write-only fix would have turned an accidentally-safe round trip
into guaranteed double-encoded corruption.

One more thing that UAT flagged and could not exercise, worth checking while in this file:
`File.ReadAllText(path, encoding)`'s underlying `StreamReader` **still auto-detects and strips a BOM**
even when an explicit encoding is passed, so a BOM'd source manifest would not behave the way CPE-1834's
reasoning assumes. Moot today because the mojibake guard asserts no repo file carries a BOM, but it is an
untested assumption sitting under the release path.

## Work Log

**2026-08-21 — fixed, tested, PR opened.** Branch `cpe-1841-release-version-regex-scope`.

### Reproduced first

Ran `origin/main`'s bump (lines 1-50 of `scripts/release.ps1`, git section snipped) under `pwsh 7.6.5`
against fixtures carrying a decoy of each shape. All three decoys were rewritten to the app version:

| manifest | decoy | before | after |
|---|---|---|---|
| `package.json` | `"someTool": { "version": "3.2.1" }` | `3.2.1` | `9.9.9` |
| `tauri.conf.json` | `"wix": { "version": "3.11.2" }` | `3.11.2` | `9.9.9` |
| `Cargo.toml` | `[dependencies.somepkg]` / `version = "1.2.3"` | `1.2.3` | `9.9.9` |

Version-shaped strings in a description or a URL were already safe under the old regex (they are values,
not `"version"` keys) — tested anyway, since the AC asks for it and nothing was guarding it.

### The fix — `scripts/release.ps1`

Replaced the three un-anchored `-replace` calls with locators that find the ONE value each bump means and
splice it in place:

- `Find-JsonTopLevelVersionValue` — walks the text tracking JSON nesting depth and string escapes, and
  matches a `"version"` **key at depth 1** (the root object) with a string value. A nested key sits at
  depth 2+ and is unreachable. An indentation rule or a `(?m)^` anchor would still hit a pretty-printed
  nested key; `ConvertFrom-Json`/`ConvertTo-Json` would reformat the whole file.
- `Find-TomlPackageVersionValue` — restricts the `^version = "..."` match to the span between the
  `[package]` header and the next `[`-headed table, which is what puts `[dependencies.somepkg]` out of
  reach. (`.NET`'s multiline `$` matches *after* the `\r` of a CRLF file, hence `\r?$` in the header
  pattern.)
- `Update-ManifestVersion` — shared: **throws unless the locator finds exactly one match**, both in the
  file as read and again in the text about to be written, and writes nothing on failure.
- New `-BumpOnly` switch: bump the three manifests and stop before `git add/commit/tag/push`. It exists so
  the tests can drive the real script rather than a re-implementation, and so a human can dry-run a bump.
  Documented in `RELEASING.md`.

One PowerShell trap worth recording: `return , $hits` (the idiomatic "don't unroll" form) hands the caller
a **one-element array wrapping the whole list**, so the `-ne 1` guard read "exactly one hit" no matter how
many there were, and then spliced at the wrong offset. Verified on Windows PowerShell 5.1 *and*
PowerShell 7; `return $hits` is correct here because the caller re-wraps with `@(...)`. Caught by the
post-write check, which is exactly what it is there for.

### Loud failure — checked, and it was the bad shape

The pre-fix script **silently succeeded**: given manifests with no version key at all, it printed
`Bumped version to 9.9.9 in package.json, tauri.conf.json, Cargo.toml` and **exited 0**, having written
all three files back byte-identical. `-replace` returns its input unchanged when nothing matches, so
there was no failure path at all. Now: `exit 1` with
`expected exactly one top-level "version" key in <path>, found 0. Refusing to write ...`, before any
write. Measured both: OLD `exit 0`, NEW `exit 1`.

### One-line diff on the REAL manifests — measured, not assumed

Ran the fixed script with `-BumpOnly` over the actual worktree manifests (`0.57.68` -> `9.9.9`),
then reverted:

```
git diff --numstat
1       1       package.json
1       1       src-tauri/Cargo.toml
1       1       src-tauri/tauri.conf.json
```

Byte-level after the bump: `loneLF=0` (CRLF intact) / `bom=False` / trailing `\r\n` intact, all three.
`md5sum` after `git checkout --` matched the pre-run hashes exactly, so nothing leaked into the branch.

### The BOM assumption — verified, and it does NOT hold repo-wide

Measured directly, on both hosts: `[System.IO.File]::ReadAllText($path, <BOM-less UTF8Encoding>)` on a
file whose bytes start `EF BB BF` returns a string whose first character is **U+007B (`{`)**, not U+FEFF.
The BOM is stripped despite the explicit encoding — CPE-1834's UAT was right. Writing that string back
with the BOM-less encoding would therefore have **deleted** the BOM: a second changed line in a supposedly
one-line bump, and an encoding change the release script was never asked to make.

The guard assumption is *half* true. A byte-level scan of all 3260 tracked files found **12 with a UTF-8
BOM — every one under `Ticketing/`**, which `mojibakeGuard.test.ts` explicitly **excludes** (as it does
`samples/`), so the guard never sees them (tracked as CPE-1784). Inside the guard's scanned set: **0**.
So "no repo file carries a BOM" is not the repo-wide invariant it reads as; it holds for the three
release manifests only because of where the exclusion boundaries fall. Confirmed all three manifests and
`release.ps1` itself are BOM-free today.

Hardened anyway rather than relying on that: `Update-ManifestVersion` now sniffs the first three raw bytes
and writes back with `UTF8Encoding($true)` if the file already had a BOM. The bump changes the version and
nothing else; whether a manifest should carry a BOM is the mojibake guard's business, not a side effect of
cutting a release. Verified both paths (BOM'd fixture keeps its BOM; BOM-less fixture gains none).

### Tests — `src/lib/releaseVersionBump.test.ts` (new, 19 tests)

Drives the **real** `scripts/release.ps1`, copied into an OS-temp scratch tree and run with `-BumpOnly`
(same tmpdir pattern `mojibakeGuard.test.ts` uses); no re-implementation of the regexes, which would stay
green while the shipped script rotted. If no PowerShell host is found the suite **throws** rather than
skipping. Each decoy gets its own test so a regression names itself:

- 3 separate decoy tests (long-form Cargo dependency pin / nested `package.json` tool version / nested
  `tauri.conf.json` wix version), plus description-string, URL-string, and `rust-version` cases.
- exactly-one-changed-line + CRLF/trailing-newline/no-BOM byte checks.
- 6 loud-failure tests: no version key in each of the three manifests, two top-level `"version"` keys,
  two `version` lines in `[package]`, and no `[package]` table at all (asserting the `found 0` message
  specifically, not just a non-zero exit).
- a red-proof block that re-runs the verbatim pre-fix `-replace` calls over the same fixtures and asserts
  every decoy IS clobbered and that the old code exits 0 having bumped nothing — proving the fixtures are
  a real trap, not decoration.

### Red-proof by actual reversion

Reverted the scoping in the working tree and re-ran, twice:

1. Reverted line: `Update-ManifestVersion -Path (Join-Path $repo "src-tauri/Cargo.toml") -NewVersion $Version -Locator 'Find-TomlPackageVersionValue' -What 'version key inside [package]'`
   back to `$cargo = $cargo -replace '(?m)^(version\s*=\s*")[^"]+(")', "`${1}$Version`$2"` (plus its
   read/write pair) -> **5 failed / 14 passed**, including "leaves a long-form Cargo dependency pin alone".
2. Reverted line: `Update-ManifestVersion -Path (Join-Path $repo "package.json") -NewVersion $Version -Locator 'Find-JsonTopLevelVersionValue' -What 'top-level "version" key'`
   back to `$pkg = $pkg -replace '("version"\s*:\s*")[^"]+(")', "`${1}$Version`$2"` -> **4 failed /
   15 passed**, including "leaves a nested tool version alone in package.json".

Both restored; suite back to 19/19.

### Gates

- `npm test` (full vitest): **321 files / 4277 tests passed**, 0 failed.
- `npm run check` (svelte-check + tsc): **0 errors, 0 warnings**.
- `src/lib/mojibakeGuard.test.ts` alone: **62 passed** (release.ps1 stays CRLF, BOM-less, valid UTF-8
  after the edit — checked at byte level as well).
- No Rust touched, so no cargo gate applies.

### Not verified

- The script's **git** half (`add`/`commit`/`tag`/`push`) is untouched and deliberately not exercised —
  no release was cut.
- CI's ubuntu runner is assumed to ship `pwsh` (it does in the GitHub-hosted image, and `release.yml`
  already uses `shell: pwsh`, though on the Windows leg). If it ever does not, this suite reds loudly
  rather than skipping — by design, but worth knowing where that red would come from.
- One local flake seen once in a combined 2-file vitest run (12/81 failed, unreproducible across 4
  subsequent runs plus two full `npm test` runs); almost certainly Defender touching the freshly-copied
  temp `.ps1`, which is a known local hazard here. Every status assertion now prints the script's
  stdout+stderr so a future occurrence is diagnosable instead of mysterious.
