---
id: CPE-1853
title: the release script bumps three of the five files that must stay version-synchronised
type: task
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-22
closed: 2026-08-22
---

## Problem

CLAUDE.md requires **five** files to carry the same version on release:

1. `package.json`
2. `src-tauri/Cargo.toml`
3. `src-tauri/tauri.conf.json`
4. `package-lock.json` — **two** places (top-level `version` and `packages[""].version`)
5. `src-tauri/Cargo.lock` — the `cross-platform-explorer` package entry

`scripts/release.ps1` bumps the first three. The two lockfiles remain manual.

CLAUDE.md already explains why 4 and 5 are the ones that get missed: **nothing fails when they drift.**
Neither build passes `--locked`, so both lockfiles are silently rewritten at build time and the stale
version never surfaces as an error. It surfaces instead as a dirty working tree the moment anyone runs
`npm install` or a local `cargo build` — which reads as unrelated noise.

It has already happened: CLAUDE.md records `package-lock.json` sitting three releases behind
(`0.57.64` vs `0.57.67`) as of 2026-08-20.

## Why now

CPE-1841 gave this script the machinery this needs. It now has a locator-plus-guard pattern that
**refuses to write unless it finds exactly one match**, and fails loudly rather than reporting success
having changed nothing. Extending that to two more files is a natural fit rather than new invention.

## Acceptance criteria

- [x] `package-lock.json`'s **both** version fields and `src-tauri/Cargo.lock`'s `cross-platform-explorer`
      entry are bumped by the same script, under the same exactly-one-match guard.
- [x] `Cargo.lock` is scoped to the **right package entry**. Every other package's version in that file is a
      dependency pin — rewriting one is precisely the defect CPE-1841 existed to fix, in a file that
      contains hundreds of them. Test with a decoy package whose version matches the app's.
- [x] `package-lock.json` needs **two** edits, not one. A test must fail if only one lands — that is the
      specific way this file goes stale.
- [x] Red-proof each locator with the minimal realistic change, observe red, revert, record the line.
- [x] Byte-level: preserve CRLF, the trailing newline, and BOM presence-or-absence per file, and keep the
      diff minimal. CPE-1841 measured `1 1` per file for the three it handles; state the expected numstat
      for the two new ones and hold it.
- [x] A single guard test asserts **all five** carry the same version after a bump, so the next file added
      to the list cannot be silently forgotten.
- [x] Say explicitly whether the build should also start passing `--locked`. That is the change that would
      make drift fail loudly on its own instead of relying on this script — out of scope to implement here,
      but the decision belongs in this ticket's record.

## Notes

Raised by the independent Reviewer during CPE-1841 as the natural home for this check, given the
loud-failure machinery that ticket introduced.

Related: CPE-1841 (the scoped locators and the exactly-one guard), CPE-1852 (the half-bumped tree that
lands in the same working directory), CPE-1834 and CPE-1842 (the encoding fixes on the same release path).

## Work Log

**2026-08-22 — fixed, tested, PR opened.** Branch `cpe-1853-lockfile-version-sync`, off `origin/main`
`dc3a0b95`. Worked in an isolated worktree under `.claude/worktrees/cpe-1853`.

### Host, stated up front

**There is no `pwsh` on this machine.** `which pwsh` → "no pwsh in (...)" (the transient dotnet-tool shim
CPE-1841 recorded is still gone); `powershell -NoProfile -Command '$PSVersionTable.PSVersion'` →
**5.1.26100.9168**, `PSEdition` **Desktop**. So **every local measurement below ran under Windows
PowerShell 5.1**, and the harness's `findPowerShellHost()` (prefers `pwsh`, falls back to `powershell`)
resolved to `powershell` here. The reproducible PowerShell 7 evidence is CI's `Frontend — type-check and
test` leg on `ubuntu-latest`, where `pwsh` is the only host and which runs this very suite. **No tool was
installed or removed** (CPE-1856's lesson: a sibling agent's `dotnet tool install`/uninstall silently
changed which interpreter a suite ran under).

Other hosts used, for the record: `cargo 1.97.0 (c980f4866 2026-06-30)`, `npm 10.9.8`, `node v22.22.3`,
all on this Windows machine.

### The fix — `scripts/release.ps1`

Two new locators, following CPE-1841's contract exactly (return **every** hit, `return $hits` with **no**
unary comma, caller insists on the declared count):

- **`Find-NpmLockVersionValues`** — returns BOTH of package-lock.json's app-version fields: the root
  object's `"version"` and `packages[""]`'s. It is CPE-1841's depth-tracking JSON walk plus a stack of
  the enclosing key at each depth, because `packages[""].version` **cannot be identified by depth
  alone** — `"node_modules/foo": { "version": "1.2.3" }` sits at the identical depth 3 and is a
  dependency pin. Only the empty-string key distinguishes the root package's own entry.
- **`Find-CargoLockPackageVersionValue`** — walks each `[[package]]` block (header to the next
  `[`-headed table) and takes `version` only from the block whose `name` is `cross-platform-explorer`.
  Scoped by **package identity**, not position. That excludes ~995 dependency pins, the top-of-file
  `version = 3` format marker (which sits before the first `[[package]]` header and is an unquoted
  integer besides), and `[[patch.unused]]` — a table that can carry the app's **own name** and must
  still not be bumped.

`New-ManifestVersionPlan` gained `-ExpectedCount` (default 1; **2** for package-lock.json). The count is
**declared per file, not inferred**, so "the locator found fewer than this file is supposed to have" is a
loud abort rather than a half-bumped lockfile. Multi-hit splices run **last-offset-first** (an earlier
splice shifts every later offset and the replacement is not the same length), old values are read off the
original text in source order, and the post-splice self-check now asserts the count **and** that *every*
hit reads back as the new version — so a splice that updated only some of its hits cannot pass. The
plan carries a `Places` count and the writer prints `(2 places)`, so package-lock.json's second edit is
visible in the release output rather than taken on trust.

`Invoke-Git add` now stages all five. Bumping a file and not committing it is the same drift one step
later.

CPE-1852's validate-all-then-write-all is untouched and now covers five files: a `Cargo.lock` that fails
the guard leaves the other four byte-identical.

### The exactly-one-match guard's message, kept English

`-ExpectedCount` is rendered as a word (`one`, `two`, numeral beyond), so CPE-1841's existing assertions
on `/expected exactly one .*found 0/` stay valid and the new ones read `expected exactly two ... found 1`.

### Expected numstat for the two NEW files, and it held

`package-lock.json` is **`2  2`** — it is the only one of the five that is not `1  1`, because it carries
the app version twice. `src-tauri/Cargo.lock` is **`1  1`**.

Measured on **throwaway copies of the real files** in a scratch git repo **inside the worktree**
(`.claude/worktrees/cpe-1853/.scratch-cpe1853/`, deleted afterwards), so no tracked file was ever written
by PowerShell. `0.57.68 -> 9.9.9`, Windows PowerShell 5.1:

```
git diff --numstat
2       2       package-lock.json
1       1       package.json
1       1       src-tauri/Cargo.lock
1       1       src-tauri/Cargo.toml
1       1       src-tauri/tauri.conf.json
```

Byte level, all five after the bump: `loneLF=0` (CRLF intact) / `bom=False` / trailing `\r\n` intact.
Literal-string occurrences of `9.9.9` after the bump: **2** in `package-lock.json`, **1** in
`src-tauri/Cargo.lock` (996 `[[package]]` blocks in that file; 995 dependency pins untouched). The
changed lines, from `git diff -U0`:

```
package-lock.json      @@ -3 +3 @@    "version": "0.57.68",      -> "9.9.9"
package-lock.json      @@ -9 +9 @@        "version": "0.57.68",  -> "9.9.9"   (packages[""])
src-tauri/Cargo.lock   @@ -1441 +1441 @@ name = "cross-platform-explorer" / version = "0.57.68" -> "9.9.9"
```

### A line-endings finding worth carrying

`loneLF=0` is a property of a **fresh checkout**, not of these files. Measured on this machine's **main**
worktree the same day: `package.json` `loneLF=49` and `src-tauri/Cargo.lock` `loneLF=10802`, while
`Cargo.toml`, `tauri.conf.json` and `package-lock.json` were `loneLF=0`. With `core.autocrlf=true`, git
checks out CRLF but `npm install` and `cargo build` rewrite their files as **LF**, and git normalises on
read so nothing shows as modified. So the bump must be endings-agnostic, not CRLF-assuming. There is a
test for exactly that: an LF-staged `Cargo.lock` comes back with **zero CR bytes** and a trailing `\n`,
while the CRLF `package-lock.json` staged in the same run stays CRLF. Splicing gives this for free —
every byte outside the located value is carried through untouched — but it was previously untested.

### Tests — `src/lib/releaseVersionBump.test.ts`: 33 → **58**

The harness now stages **all five** files (release.ps1 plans every one before writing any, so a missing
lockfile would abort in the plan phase and red every unrelated case). `Manifests` gained optional
`lock` / `cargoLock` defaulting to the new decoy fixtures, and `FILE_KEYS` / `FILE_PATHS` /
`VERSION_PLACES` replace the hard-coded three-element arrays, so a sixth file is one entry rather than a
dozen literals. `BumpOptions` gained `lf?: FileKey`.

Both new fixtures carry a decoy **whose version is byte-identical to the app's** (`0.1.0`):

- `package-lock.json`: `node_modules/decoy` at the same depth and same value as `packages[""]`;
  `node_modules/other` with a version-shaped funding URL; `"lockfileVersion": 3` as an unquoted number on
  a version-shaped root key; and a `"deprecated"` string containing `{`, `}` and an escaped `"`, which
  makes a walker that tracks nesting without consuming string tokens miscount depth from that point on.
- `Cargo.lock`: `decoy-crate` at exactly the app's version; `serde` as an ordinary pin;
  `[[patch.unused]]` carrying the app's **own name**; the top-of-file `version = 3`; and a `[metadata]`
  table with version-shaped keys.

New blocks: package-lock's two fields (6 tests), Cargo.lock scoping (7), loud failures + atomicity for
files 4 and 5 (6), the all-five guard (3), and the CPE-1853 red-proof (3). The pre-existing CRLF/BOM
loops were widened from three files to `FILE_KEYS`, so the BOM-preserve path is now asserted on all five.

**The all-five guard is about the LIST, not the files.** One test bumps once and asserts every one of the
five reads back at the new version, via per-file readers keyed by `FILE_KEYS` (and cross-checks the count
of places found against `VERSION_PLACES`, so a future edit cannot quietly drop package-lock's second
field from the expected literal). Two more read **CLAUDE.md's own numbered list** and release.ps1's text,
and assert that CLAUDE.md's five paths, release.ps1's five `New-ManifestVersionPlan` call sites, its
`Invoke-Git add` line, and this file's `FILE_PATHS` all name the same set. Add a sixth file to CLAUDE.md
and forget the script — or bump it and forget to commit it — and this reds. That is precisely how files 4
and 5 came to be missed for three releases.

### Red-proof by actual reversion — three, each observed red then restored

1. **Cargo.lock locator un-scoped.** `scripts/release.ps1`, in `Find-CargoLockPackageVersionValue`,
   reverted `if ($nameMatch.Success -and $nameMatch.Groups[1].Value -eq $PackageName) {` to
   `if ($hits.Count -eq 0) {` (i.e. the plausible naive scoping: take the first `[[package]]` block).
   → **7 failed / 51 passed**, including *"leaves a decoy [[package]] whose version EXACTLY matches the
   app's alone (decoy-crate)"* and *"bumps the cross-platform-explorer [[package]] entry"*.
2. **package-lock bumped top-level only** — the specific stale shape the ticket names. Reverted the plan
   line to `-Locator 'Find-JsonTopLevelVersionValue' -What 'top-level "version" key'` with no
   `-ExpectedCount 2`. → **6 failed / 52 passed**, including *"bumps the ROOT version AND packages[\"\"]
   -- failing if only one of the two lands"* and *"exits non-zero when package-lock.json carries the
   version in only ONE place"*.
3. **The two lockfiles dropped from the release commit.** Reverted `Invoke-Git add` to the three-file
   form. → **exactly 1 failed**, *"release.ps1 stages all five in the release commit"*, with
   `AssertionError: package-lock.json is bumped but never staged for the release commit`.

All three restored; `scripts/release.ps1` md5 `13456c678bb682296fc75a062d7e8439`, verified byte-identical
to the pre-red-proof snapshot by `diff`.

Mechanised alongside them (a green there does **not** mean the shipped script is correct — the 51 tests
that drive `scripts/release.ps1` itself carry that; 7 of the 58 are red-proof against frozen or
hypothetical scripts): `NAIVE_LOCKFILE_SCRIPT` and
`TOP_LEVEL_ONLY_SCRIPT`, the two mistakes a minimal extension actually makes, run over the same fixtures
to prove the fixtures are traps rather than decoration. Unlike CPE-1841's `PRE_FIX_SCRIPT` these are
**not** transcriptions of anything that shipped — there was no lockfile code to transcribe, which is the
ticket. Measured: the naive un-scoped replace rewrites `node_modules/decoy`, `node_modules/other`,
`decoy-crate`, `serde` **and** the `[[patch.unused]]` entry to the app version; the top-level-only
variant moves the root `"version"` and leaves `packages[""]` at `0.1.0`.

### `--locked`: the decision, with measurements

**Recommendation: YES for the Rust builds, and there is nothing equivalent to add on the npm side.**
Out of scope to implement here, as the ticket says; it wants a follow-up ticket, because it will also
turn any uncommitted dependency-graph change into a CI red (which is the point, and is already required
by the repo's "multiple independent Cargo.lock files" rule).

Measured on this machine (cargo 1.97.0, npm 10.9.8), on throwaway crates in the worktree:

| command | Cargo.toml / package.json | lockfile | result |
|---|---|---|---|
| `cargo build --locked` | `0.2.0` | `0.1.0` | **exit 101** — `error: cannot update the lock file ... because --locked was passed to prevent this` |
| `cargo build` (today's behaviour) | `0.2.0` | `0.1.0` | **exit 0**, and it **silently rewrote** the lock entry to `0.2.0` |
| `npm ci` | `0.2.0` | `0.1.0` (both fields) | **exit 0**, `up to date`, lock **left stale** at `0.1.0` |

So: `--locked` converts Cargo.lock version drift from an invisible silent rewrite into a hard, loud build
failure that does not depend on this script at all — exactly the property CLAUDE.md says is missing. That
is worth having as defence in depth even with this ticket's fix in place, because the fix only helps when
a release goes through `release.ps1`.

`npm ci` is the npm analogue and is **already used everywhere** in CI
(`.github/workflows/ci.yml:174,231`, `gui-smoke.yml`, `release*.yml`), and the measurement above shows it
neither fails on nor repairs a `version`-field drift — it validates the dependency graph, not the version.
That is the direct explanation for how `package-lock.json` sat three releases behind through many green CI
runs. **There is no npm flag to add**; for `package-lock.json`, this script plus the all-five guard test
**is** the mechanism.

### Docs

`RELEASING.md`: "three files" → five throughout; the `-BumpOnly` recipe's expected numstat now says
`1  1` for four and `2  2` for package-lock.json, and the `git checkout --` cleanup line lists all five
(three files left behind was already the hazard; five is more so). Added what each locator is scoped to,
and a paragraph on why the script has to carry this rather than a build check — with the `--locked` /
`npm ci` finding.

### Gates (all local runs on Windows PowerShell 5.1 / node v22.22.3)

- `npx vitest run` (full): **325 files / 4365 tests passed**, 0 failed.
- `npm run check` (svelte-check + tsc): **0 errors, 0 warnings**.
- `src/lib/releaseVersionBump.test.ts` alone: **58 passed** (was 33; +25).
- No Rust source touched, so no cargo gate applies (the cargo runs above were throwaway crates for the
  `--locked` measurement, deleted afterwards).
- Every edited file after every edit: `loneLF=0` (CRLF intact), no BOM, trailing `\r\n` intact,
  `rawESC=0`; the test file carries **zero** non-ASCII bytes, and `release.ps1`'s 6 non-ASCII bytes are
  the two pre-existing em dashes, unchanged.

### A transport hazard I hit, recorded

A `\\s` written into a **quoted** bash heredoc (`<<'TSEOF'`) arrived in the file as `\s` — and inside a
TypeScript template literal JS drops an unrecognised escape, so `\s` became a bare `s`. The two new
red-proof scripts' regexes therefore matched nothing, the scripts exited 0 having changed nothing, and
**3 tests failed by staying at the old version** — which reads exactly like "the fixture is not a trap"
rather than "the fixture was never tested". Caught because those 3 were the only reds in an otherwise
green run. Fixed with the Edit tool (which does not re-encode) and verified by evaluating the template
literal to real PowerShell before re-running. Same family as the raw-`0x1B` transport bug CPE-1852 hit in
this file: **backslash-heavy strings must not be written through a heredoc; use the Edit tool and verify
the resolved bytes.**

### Not verified

- **PowerShell 7 locally** — impossible here, there is no `pwsh` on this machine and installing one is
  the machine-global change that made CPE-1841's numbers unreproducible and cost CPE-1856. CI's ubuntu
  leg carries it; the suite reds loudly rather than skipping if a runner ever loses its host.
- **The `--locked` change itself** — measured on throwaway crates, deliberately not implemented (the
  ticket scopes it to a decision). Its effect on the real `tauri build` / CI matrix is untested.
- **The git half** of the script (`add`/`commit`/`tag`/`push`) is untouched apart from the two added
  paths on the `add` line, and is never exercised on a real repo. The added paths are asserted by a test
  that reads the script's text, not by running it. **No release was cut.**
- **A BOM'd real lockfile** — the BOM-preserve path is exercised on the fixtures (all five), not on a
  real 122KB/258KB lockfile; none carries a BOM today.

---

## Work Log — round 2 (Reviewer APPROVED; two guards made honest)

PR #998 was **APPROVED** by the independent Reviewer, which reproduced everything and then strengthened
two of the claims beyond what I had measured. Round 2 fixes the two items it raised. Both are about a
**guard being honest**, not about the fix — the bump itself was green everywhere it attacked it.

### What the Reviewer proved that I had only argued

- **The depth-collision claim is not a rationalisation.** It ran a depth-3-only variant of the walk over
  the **real** `package-lock.json` and got **248 hits** against this locator's 2. Depth alone is off by
  246. It also attacked the key stack with a package literally named `""` nested deeper, a
  `packages[""]` inside `packages[""]`, a `"packages"` object under a different root key both before and
  after the root version, keys ending in an escaped backslash, a value carrying `{`/`}`/`\"`, a one-line
  file, and the lockfileVersion-2 shape where both the root `packages` map and the legacy `dependencies`
  tree carry their own `""` entry. **All returned exactly 2.**
- **Last-offset-first splicing is load-bearing, not ordering luck.** Splicing FORWARD yields
  `"version": "0.9.9.9` — the closing quote and comma eaten, i.e. broken JSON — and the post-splice
  self-check rejects that independently. Defence in depth, both halves doing work.

### 1. The CLAUDE.md ratchet depended on formatting, silently

`releaseVersionBump.test.ts` read CLAUDE.md with `/^\d+\.\s+`([^`]+)`/gm`, which only sees list items
whose path is **backticked**. A sixth entry written `6. the sidecar manifest` is invisible to it, and the
ratchet stays green while the script bumps five of six — the same "passes because it never saw the thing"
shape this run has been deleting.

Now it takes **every** numbered item first (`/^\d+\.[ \t]+(.*)$/gm`), then insists each one starts with a
backticked path, failing with the offending text if not. Also reds if the section's numbered list
disappears entirely, and if a second ordered list appears in that section — correct, because the test
would no longer know which list is the manifest list.

**Red-proof:** added `6. the sidecar manifest` to CLAUDE.md's list and re-ran → **exactly 1 failed**,
*"release.ps1 plans exactly the files CLAUDE.md's five-files-in-sync list names"*, with
`expected [ 'the sidecar manifest' ] to deeply equal []`. Run against the **same** file, the old regex
saw **5 items** and would have stayed green; the new parse sees **6** and names the unreadable one.
CLAUDE.md restored and md5-verified byte-exact (`2e983f2c3aa736e9a2fa889f9b587b40`); it does not appear
in this branch's diff.

### 2. The two-hit guard said "two hits", not "one of each"

`-ExpectedCount 2` is also satisfied by two duplicate root-level `"version"` keys with **no `packages`
object at all** — the Reviewer verified it, and so did I: with the kind check removed the script
**exits 0** on that file and bumps it. Unreachable from any npm output (npm never emits duplicate keys,
and a real lockfile always has `packages`), so this is completeness rather than a live defect.

`Find-NpmLockVersionValues` now tags each hit with its `Kind` (`root "version"` /
`packages[""]."version"`), and `New-ManifestVersionPlan` gained `-ExpectedKinds`, asserted against the
sorted kind signature **both** on the hits as read and on the post-splice re-scan. An untagged hit
renders as `(untagged)`, so asking for kinds from a locator that does not tag fails loudly rather than
matching an empty expectation. The other four files pass no kinds and are unchanged.

New message when the count is right and the shape is not:

```
release.ps1: expected the app "version" keys in <path> to be exactly [packages[""]."version" + root
"version"], found [root "version" + root "version"]. No manifest was written -- the right NUMBER of
version values is not the same as the right ONES, ...
```

**Red-proof:** removed `-ExpectedKinds` from the plan call and re-ran → **exactly 1 failed**, *"exits
non-zero on TWO hits of the WRONG KIND (duplicate root keys, no packages object)"*, with
`AssertionError: release.ps1 exit=0` — the Reviewer's finding reproduced verbatim. Restored;
`scripts/release.ps1` md5 `3804ce4fd10d821e2dc679be0c8a4521`.

The new test asserts the **kind** message fires and the **count** message does not
(`not.toMatch(/found 2\./)`), so it cannot pass for the wrong reason.

### 3. The datum that completes the `--locked` story

The Reviewer added the piece both of us were missing, and I re-measured it here (npm 10.9.8) rather than
take it on trust — **`npm install` on the same stale state exits 0 and SILENTLY REPAIRS both fields**:

| command | package.json | lock before | exit | lock after |
|---|---|---|---|---|
| `npm ci` | `0.2.0` | `0.1.0` ×2 | 0 | `0.1.0` ×2 — **neither fails nor fixes** |
| `npm install` | `0.2.0` | `0.1.0` ×2 | 0 | `0.2.0` ×2 — **silently repaired** |
| `cargo build` | `0.2.0` | `0.1.0` | 0 | `0.2.0` — silently repaired |
| `cargo build --locked` | `0.2.0` | `0.1.0` | **101** | unchanged — **loud** |

So the full mechanism, which is a better explanation than either of us had on its own: **CI's `npm ci`
neither fails nor fixes; a developer's `npm install` fixes it without telling anyone; and the repair
surfaces only as a dirty tree that reads as noise** — and gets discarded along with real work, putting
the file straight back where it was. That is precisely how `package-lock.json` sat three releases behind
through green CI. It also sharpens the recommendation: the drift is not merely undetected, it is
*repeatedly and invisibly repaired-then-discarded*, so nothing accumulates evidence of it.

### 4. The line-ending finding, confirmed against the tree the script actually runs in

The Reviewer went further than I did: rather than a fresh-checkout copy set, it ran the real script
against copies of the **live main-worktree bytes** (mixed LF/CRLF). I reproduced that here. Before, and
byte-identical after:

| file | loneLF before | loneLF after | BOM | tail |
|---|---|---|---|---|
| `package.json` | **49** | **49** | none | `}\n` |
| `src-tauri/tauri.conf.json` | 0 | 0 | none | `\r\n` |
| `src-tauri/Cargo.toml` | 0 | 0 | none | `\r\n` |
| `package-lock.json` | 0 | 0 | none | `\r\n` |
| `src-tauri/Cargo.lock` | **10802** | **10802** | none | `]\n` |

with the same numstat table (`2  2` for package-lock.json, `1  1` for the other four). The Reviewer's
conclusion, worth quoting because it is stronger than how I put it: **in the tree where `release.ps1`
actually runs, two of the five files are LF, so a CRLF-assuming bump would have corrupted them
silently.** It is not a hypothetical property of a fresh checkout.

### Recorded, no action (the Reviewer's three, agreed and left)

- **The `Invoke-Git add` guard is a text read of the script, not a git run.** It closes the
  bumped-but-unstaged defect at the level a text assertion can reach; a broken `Invoke-Git` itself would
  still ship green. Exercising it needs a real repo and a real commit, which is the half of this script
  deliberately never run under test.
- **The suite stages fixtures into `os.tmpdir()`**, outside the project — inherited from CPE-1841's
  harness (which copied `mojibakeGuard.test.ts`'s pattern) and against the standing keep-FS-work-inside-
  the-project rule. Not changed here: it is a whole-file harness move, it would touch every one of the
  59 tests, and it is orthogonal to both items this round is scoped to. Note that **my own** measurement
  scratch dirs were all inside the worktree and are deleted.
- **`release.ps1:236` does `$Text.Substring($sectionStart)` inside a loop over 996 headers** — O(n²) on
  the Cargo.lock scan. Measured by the Reviewer at **152 ms** for Cargo.lock and **47 ms** for
  package-lock. Irrelevant at this size on a script run a few times a month; would matter around 10x.

### Round-2 gates (local, Windows PowerShell 5.1 — still no `pwsh` on this machine)

- `npx vitest run` (full): **325 files / 4366 tests passed**, 0 failed (was 4365; +1 wrong-kind test).
- `npm run check`: **0 errors, 0 warnings**.
- `src/lib/releaseVersionBump.test.ts` alone: **59 passed** (was 58).
- `releaseVersionBump` + `mojibakeGuard` together: **121 passed**.
- Real-file numstat re-measured after the script edit, on the **live mixed-EOL** bytes: `2  2` for
  `package-lock.json`, `1  1` for the other four; line endings, BOM absence and trailing bytes all
  preserved exactly as staged (table above).
- Both edited files after every edit: `loneLF=0`, `rawESC=0`, no BOM, trailing `\r\n` intact; the test
  file still carries **zero** non-ASCII bytes.
- `CLAUDE.md` untouched by this branch — md5-verified byte-exact after the red-proof.
