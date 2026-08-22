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

Ran `origin/main`'s bump (lines 1-50 of `scripts/release.ps1`, git section snipped) against fixtures
carrying a decoy of each shape. All three decoys were rewritten to the app version:

> **Host note (added in round 2 — read this before trusting any host claim below).** This ran under
> PowerShell 7.6.5 from a **transient** `dotnet tool --tool-path` shim at `~/.dotnet/tools/pwsh` that a
> concurrent sibling worker installed and then removed at 22:14. It is **gone from this machine now** and
> these runs are **not locally reproducible**. Everything after 22:14 ran on Windows PowerShell 5.1
> (5.1.26100.9168). The reproducible PowerShell 7 evidence is the green `Frontend — type-check and test`
> CI leg on ubuntu-latest, where `pwsh` is the only host. Full account in "Work Log — round 2" below.

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
many there were, and then spliced at the wrong offset. Verified on Windows PowerShell 5.1 (reproducible
today) *and* on the transient PowerShell 7.6.5 described in the host note above (not reproducible today);
`return $hits` is correct here because the caller re-wraps with `@(...)`. This is core-engine pipeline
unrolling semantics, identical across hosts, and the Reviewer independently reproduced it. Caught by the
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

Measured directly, on both hosts (5.1 reproducible today, 7.6.5 on the transient shim — see the host note
above; this is `StreamReader` BOM auto-detection, which does not vary by host, and the Reviewer confirmed
the behaviour independently): `[System.IO.File]::ReadAllText($path, <BOM-less UTF8Encoding>)` on a
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
  **[Round 2: no longer an assumption — the green `Frontend — type-check and test` leg on ubuntu-latest
  runs these tests, so `pwsh` is confirmed present and the suite is confirmed green under PowerShell 7.]**
- One local flake seen once in a combined 2-file vitest run (12/81 failed, unreproducible across 4
  subsequent runs plus two full `npm test` runs); almost certainly Defender touching the freshly-copied
  temp `.ps1`, which is a known local hazard here. Every status assertion now prints the script's
  stdout+stderr so a future occurrence is diagnosable instead of mysterious.
  **[Superseded in round 2 — the real cause was found. See below.]**

---

## Work Log — round 2 (post-review)

PR #988 was APPROVED by the independent Reviewer (18/18 CI checks green, no blocking findings; it
attacked the parsers with escaped backslashes, one-line nested objects, `[package.metadata.wix]` spans,
commented-out version lines and a 40-case fuzz, all of which held). Round 2 addresses its three
follow-ups plus the corrections below.

### 1. CORRECTION — which PowerShell host I actually measured on

The Reviewer was right to challenge this, and right about what it found: **there is no `pwsh` on this
machine now.** The correction is narrower than "the claim was fabricated", and the full truth is worth
recording because it also explains the flake.

What actually happened, with the evidence:

- Early in this ticket, `which pwsh` resolved to **`/c/Users/Stewart Rogers/.dotnet/tools/pwsh`** and
  `pwsh -v` printed **`PowerShell 7.6.5`**. That is a `dotnet tool --tool-path` shim in the user profile —
  not `C:\Program Files\PowerShell`, not a winget package, and not in the uninstall registry, which is
  exactly why the Reviewer's search found nothing. The measurements were real, on a real PowerShell 7.6.5.
- **I did not install it.** A concurrent sibling worker did (the coordinator notes another worker used
  `dotnet tool install --tool-path` today), and that worker then **removed** it mid-run.
- Timestamps confirm the removal: `~/.dotnet/tools` mtime **2026-08-21 22:14:12**, `.store` **22:14:13**,
  `.store/.stage` **22:14:14**. `pwsh.exe` is gone; `dotnet tool list --global` now shows only
  `dotnet-dump` and `dotnet-ildasm`.

So the honest statement, replacing the earlier one:

- The **reproduction of the bug**, the **BOM strip measurement**, the **`return , $hits` measurement**,
  the **numstat measurement**, and the vitest runs at 22:05 / 22:08 / 22:10 / 22:12 ran under
  **PowerShell 7.6.5** (that transient dotnet-tool shim) — genuinely measured, but **not reproducible on
  this machine today**.
- The **Windows PowerShell 5.1** halves of those same measurements ran under `powershell`
  (**5.1.26100.9168**, still present) and **are** reproducible.
- The standing, reproducible **PowerShell 7 evidence is CI**: `Frontend — type-check and test` runs
  `npm test` on `ubuntu-latest` (`.github/workflows/ci.yml:163,180`), where `pwsh` is the only host, and
  it is green on this head. That is what the claim should have rested on, and it is what it rests on now.
- Everything after 22:14 — including the final full `npm test` and every round-2 run — used **5.1**.
  Both hosts have therefore now run the full suite green locally, plus pwsh on CI.

Both the PR body and the Work Log above have been rewritten accordingly.

### 2. The flake: not Defender — pwsh was uninstalled out from under the running suite

Retracting the Defender hypothesis. The failing run **started at 22:14:04**; `~/.dotnet/tools` was
modified at **22:14:12–22:14:14**. `findPowerShellHost()` probed `pwsh` successfully at suite start, then
individual spawns hit a shim whose payload was being deleted. Timeline:

| time | run | host | result |
|---|---|---|---|
| 22:05:24 | new file alone | pwsh 7.6.5 | 19/19 |
| 22:08:06 | + mojibake guard | pwsh 7.6.5 | 81/81 |
| 22:10:06 | full `npm test` | pwsh 7.6.5 | 4277/4277 |
| 22:12:07 | new file alone | pwsh 7.6.5 | 19/19 |
| **22:14:04** | + mojibake guard | **pwsh being deleted** | **12 failed / 69 passed** |
| 22:15:30 onward | all runs | powershell 5.1 | green |

The Reviewer's alternative (default 5000ms per-test timeout, never overridden, versus 19 separate
PowerShell spawns) is a real latent hazard even though it was not this failure — so it is fixed anyway,
below. The lesson recorded for the machine-sharing rules: a sibling worker installing and removing a
**global tool** can red a concurrent worker's suite, and it leaves no trace once removed.

### 3. The BOM-preserve path now ships tested (was: shipped untested)

Correct finding — `$hadBom = $true` had no guard; the suite only asserted a BOM is never *added*. Six new
tests in `src/lib/releaseVersionBump.test.ts`:

- still bumps the top-level version in a BOM'd manifest
- leaves `EF BB BF` at offset 0 in all three manifests instead of stripping it
- adds exactly **one** BOM, not a second in front of the first (the `UTF8Encoding($true)` preamble plus a
  surviving U+FEFF would give `EF BB BF EF BB BF`)
- carries a non-ASCII payload through byte-exact — accented Latin, an em dash, CJK, and an astral emoji —
  asserting both that `E2 80 94` survives and that the CP1252 byte `0x97` never appears (the CPE-1834
  failure mode, which is a *different* bug from BOM handling and would be hidden by a BOM-only assertion)
- still exactly one changed line, CRLF and trailing newline intact
- **does NOT** add a BOM to a manifest that never had one — the conditional's other arm, without which a
  `$hadBom` stuck at `$true` keeps all five above green

Red-proof: reverted `[System.IO.File]::WriteAllText($Path, $updated, $writeEncoding)` (release.ps1:203)
to `$utf8NoBom` → **3 failed / 22 passed** (BOM-at-offset-0, exactly-one-BOM, one-changed-line). The
"does NOT add a BOM" case correctly stayed green. Restored; 25/25.

### 4. `RELEASING.md` — the dry run now tells you to clean up

Added `git checkout -- package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml` after the
`-BumpOnly` / `git diff --numstat` recipe, with a line saying why: three modified manifests left behind
are precisely the dirty-tree-reads-as-noise hazard CLAUDE.md already records.

### 5. Nine redundant process spawns removed, and an explicit timeout

Hoisted the shared `runBump(ALL_DECOYS, NEW)` into a `beforeAll` (nine identical spawns producing nine
byte-identical results), and the red-proof block's three likewise. Added
`vi.setConfig({ testTimeout: 60_000, hookTimeout: 120_000 })` — vitest's 5000ms default is not overridden
in `vite.config.ts` and is sized for pure-module tests, not a suite that spawns PowerShell. In-test time
fell from ~9.6s to ~5.9s for the original cases despite six new tests.

### Recorded, not fixed

**The mechanised red-proof is load-bearing but narrow.** `PRE_FIX_SCRIPT` is a frozen transcription of
one historical script (the Reviewer diffed it against `git show main:scripts/release.ps1` — all three
`-replace` expressions verbatim). It proves each decoy is a shape the old code genuinely ate, so the
"leaves X alone" tests cannot be vacuously green. It does **not** guard against a differently-broken
future script, which would leave it green; the other 19 tests, which drive the actual
`scripts/release.ps1`, carry that. Now said explicitly in the block's header comment.

**`main` already carried a false comment about this exact scoping.** `scripts/release.ps1:43` on `main`
reads `# 3. src-tauri/Cargo.toml  (only the first [package] version line)` above a regex that is neither
scoped to `[package]` nor limited to the first match. Two false claims in one line, sitting in this file
the whole time — the same false-comment shape as CPE-1824's blocker. It sharpens the ticket's story: the
scoping was believed to exist, which is why nobody looked.

**TOML locator bounds — all verified here, none able to silently clobber.** Re-ran each case myself
against the shipped script under PowerShell 5.1:

| case | exit | outcome |
|---|---|---|
| `[package]` declared twice | 0 | bumps only the first; the second's `7.7.7` untouched (invalid TOML anyway — cargo rejects it) |
| inline `package = { version = "..." }` | 1 | `found 0`, nothing written |
| `[ package ]` with inner spaces | 1 | `found 0` — would abort a release, but loudly |
| multi-line basic string containing a `version` line | 1 | `found 2` — false positive, still loud |
| commented-out `# version = "5.5.5"` above the real one | 0 | correctly skipped; real version bumped |
| `[package.metadata.wix]` after `[package]` | 0 | terminates the span; wix `3.11.2` untouched |

**Filed separately by the coordinator, deliberately not widened into this PR:** the half-bumped tree when
the third manifest fails the guard (each file is written as it goes, so a `Cargo.toml` failure leaves the
first two already at the new version — strictly better than the old silent pass, and the per-file
`path: old -> new` lines disclose it, but a validate-all-then-write-all pass would close it); and the
script covering only 3 of CLAUDE.md's 5 sync-required files (`package-lock.json` ×2 and
`src-tauri/Cargo.lock` stay manual).

### A hazard I hit and want recorded

`sed -i` on a CRLF file **rewrote `scripts/release.ps1` to LF and dropped its trailing newline** while I
was staging the red-proof revert. Caught immediately by the standing `git diff --numstat` +
byte-check habit (`loneLF=244`, `trailCRLF=False`) and restored byte-exact from HEAD (md5 verified). The
existing rule is "no PowerShell writes to repo files"; `sed -i` on a CRLF file belongs next to it. The
Edit tool and byte-level python splices both preserve line endings; `sed -i` does not.

Also: a `python` heredoc whose `open(p, "w")` throws mid-write **truncates the file to 0 bytes first**.
That happened once here (a transport-mangled escape produced a lone surrogate). Recovered from HEAD; the
round-2 edit script now validates and encodes the whole string, then writes via a temp file and
`os.replace`, so a failure can never leave a truncated source file behind.

### Round-2 gates

- `npx vitest run` (full): **321 files / 4283 tests passed**, 0 failed (was 4277; +6 BOM tests).
- `npm run check`: **0 errors, 0 warnings**.
- `src/lib/releaseVersionBump.test.ts` alone: **25 passed**.
- Real-manifest numstat re-measured after every round-2 edit, this time explicitly under **Windows
  PowerShell 5.1**: `1  1` for each of the three, `loneLF=0`, no BOM, trailing CRLF intact; manifests
  restored and md5-verified.
- `scripts/release.ps1` is byte-identical to the reviewed commit (md5 `d3f20b44ccbcfbcc980907a734c01043`);
  round 2 changed only the test file and `RELEASING.md`.
