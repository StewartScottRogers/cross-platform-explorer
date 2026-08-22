---
id: CPE-1852
title: a failure on the third manifest leaves a half-bumped tree while the message says "refusing to write"
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-22
closed: 2026-08-22
---

## Problem

`scripts/release.ps1`'s `Update-ManifestVersion` writes each manifest as it goes (`:203`). CPE-1841 added
a loud guard that refuses to write when a manifest does not carry exactly one version key — but the guard
fires **per file**, after earlier files are already on disk.

Measured by the independent Reviewer during CPE-1841:

```
package.json        -> 9.9.9   (written)
tauri.conf.json     -> 9.9.9   (written)
Cargo.toml          -> 0.1.0   (guard fired)
exit 1, "expected exactly one ... found 0. Refusing to write ..."
```

The message is true of the file it names and **reads as though nothing was written**. Two of the five
version-synchronised files are now bumped, on disk, uncommitted.

## Why Medium, not High

This is strictly **better** than what CPE-1841 replaced: the old script wrote all three and reported
success having changed nothing. The abort happens before any `git` call, so no tag and no push, and the
per-file `path: old -> new` lines do disclose what landed to a reader who looks.

But it lands squarely in the hazard CLAUDE.md already records by name — a dirty working tree after a
release operation reads as unrelated noise and gets committed by accident or discarded along with real
work. That is exactly how `package-lock.json` ended up three releases behind.

## Acceptance criteria

- [ ] Either validate all manifests **before writing any**, then write all of them; or name the
      already-written files in the failure so the message stops implying nothing changed. Prefer
      validate-all-then-write-all — it makes the operation atomic in the way the message already claims.
- [ ] The failure message must not say "refusing to write" while files have been written. Whatever wording
      results has to be true of the whole run, not of one file.
- [ ] A test stages a manifest set where the **third** file fails and asserts the first two are unchanged
      on disk. Red-proof it against the current behaviour — it must fail today.
- [ ] Check the same shape for `-BumpOnly` and for the full release path; they share the writer.
- [ ] Preserve everything CPE-1841 measured: exactly `1 1` per file on `git diff --numstat`, CRLF intact,
      trailing newline intact, no BOM added, BOM preserved where one was already present.

## Notes

Found by the independent Reviewer during CPE-1841, which correctly did not absorb it — that ticket's scope
was the unscoped version regex, and this is a separate transactional property.

Read CPE-1841's Work Log first. It carries two things worth not re-deriving: the `return , $hits` trap
(the comma operator hands the caller a one-element array wrapping the whole list, so an "exactly one match"
guard reads as satisfied regardless of the real count), and the measured byte-level round-trip that any
change here must not regress.

Related: CPE-1841 (the guard that fires), CPE-1853 (the two lockfiles this script still does not touch).

## Work Log

**2026-08-22 — fixed, tested, PR opened.** Branch `cpe-1852-atomic-manifest-bump`, off `origin/main`
`0891b1c4`. Worked in an isolated worktree under `.claude/worktrees/cpe-1852`.

### Host, stated up front

**There is no `pwsh` on this machine.** `which pwsh` → not found (the transient dotnet-tool shim CPE-1841
recorded is gone); `powershell -NoProfile -Command '$PSVersionTable.PSVersion'` → **5.1.26100.9168**.
So **every local measurement below ran under Windows PowerShell 5.1**, and the test harness's
`findPowerShellHost()` (which prefers `pwsh`, falls back to `powershell`) resolved to `powershell` here.
The reproducible PowerShell 7 evidence is CI's `Frontend — type-check and test` leg on `ubuntu-latest`,
where `pwsh` is the only host — that leg runs this very suite. No tool was installed or removed.

### The fix — validate all, then write all

`Update-ManifestVersion` did read → validate → **write**, once per manifest, in a straight line. Split in
two:

- **`New-ManifestVersionPlan`** — read, BOM-sniff, locate, count, splice, re-check through the same
  locator, and `return` a plan hashtable `@{ Path; Old; New; Text; Encoding }`. **Touches no disk.**
- **`Write-ManifestVersionPlan`** — writes one already-validated plan and prints its `path: old -> new`.

The call site builds `$plans = @(plan1; plan2; plan3)` and only then loops the writes. A throw from any
plan happens with nothing written, which is what makes the operation atomic in the way the message
already claimed. A hashtable (not a `PSCustomObject`) is returned deliberately: PowerShell emits it as
one object, so the caller's array subexpression sees exactly three elements.

### The message now true of the whole run

Was: `... found 0. Refusing to write -- a manifest that no longer matches must fail ...`
Now: `... found 0. No manifest was written -- every manifest is validated before any is written, so the
working tree is exactly as it was. A manifest that no longer matches must fail the release loudly, not be
written back unchanged and reported as bumped.`

The post-splice self-check message likewise changed `Nothing was written.` → `No manifest was written.`,
and it is now a *splice* check (it runs before the write, not after it), so it is named one.

**The residual I/O case is reported honestly rather than pretended away.** Once every plan validates, a
write can still fail on a locked or read-only file. No pure-PowerShell scheme makes three
`WriteAllText` calls atomic, so the write loop catches and reports `PARTIAL BUMP. Already written at
vX.Y.Z: <paths>` with a revert instruction — true of the run, and it names the files. That path is
untested (see "Not verified").

### Red-proof, against the current behaviour, before the fix

New `describe` block "release.ps1 leaves no half-bumped tree when a LATER manifest fails (CPE-1852)",
6 tests, run against the **unfixed** script:

```
Tests  5 failed | 26 passed (31)
```

The five that red are exactly the atomicity claims — third-manifest bytes, no phantom `old -> new` lines,
whole-run wording, second-manifest position, and the full (non-`-BumpOnly`) path. The sixth ("still
writes all three together on the success path") correctly stayed green: it is the companion arm that
stops "write nothing, ever" from being a passing fix. After the fix: **31 passed / 31**.

The failing-run stdout captured during the red proof is the ticket's exact reproduction:

```
Releasing v9.9.9...
  ...\package.json: 0.1.0 -> 9.9.9
  ...\src-tauri\tauri.conf.json: 0.1.0 -> 9.9.9
release.ps1: expected exactly one version key inside [package] in ...\Cargo.toml, found 0. Refusing to write ...
```

### `-BumpOnly` and the full release path — both covered

`runBump` grew a `bumpOnly?: boolean` option (default `true`, so every existing case is unchanged). One
test drives the script **without** `-BumpOnly` over a fixture whose third manifest fails, asserting the
first two are byte-identical and that nothing git-shaped ran. Safe by construction: validation aborts
before the first `git` call, and the scratch tree is an OS temp directory that is not a repo at all, so a
regression reaching `git add` would die there rather than commit anything. That is the point of the test —
the atomicity is a property of the shared writer, not of the dry-run switch.

### Measured on the REAL manifests (throwaway copies, never the tracked files)

Copied the worktree's actual `package.json` / `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` and
`scripts/release.ps1` into a scratch git repo **inside the worktree**
(`.claude/worktrees/cpe-1852/.scratch-cpe1852/`, deleted afterwards), so no tracked file was ever written
by PowerShell.

Success path, `0.57.68 -> 9.9.9`, Windows PowerShell 5.1:

```
git diff --numstat
1       1       package.json
1       1       src-tauri/Cargo.toml
1       1       src-tauri/tauri.conf.json
```

Byte level, all three: `loneLF=0` (CRLF intact) / `bom=False` / trailing `\r\n` intact.

Failure path on the same real manifests (`[package]`'s version line deleted, so Cargo.toml fails):

| script | package.json md5 | tauri.conf.json md5 | message |
|---|---|---|---|
| before (`origin/main`) | `1d81e357…` → **`32b3d78b…`** | `65cb42a2…` → **`96d9261d…`** | "Refusing to write" |
| after (this branch) | `1d81e357…` → `1d81e357…` | `65cb42a2…` → `65cb42a2…` | "No manifest was written" |

The old script printed both `old -> new` lines and left `numstat` showing all three files modified. The
new one prints neither and leaves only the file I had deliberately broken.

`scripts/release.ps1` itself after editing (Edit tool, no `sed -i`, no PowerShell write): `loneLF=0`,
`bom=False`, trailing `\r\n` intact.

### CPE-1841's measurements — all preserved

The BOM-preserve path, the `$hadBom` sniff, the `return $hits` (no unary comma) contract, and every
locator are untouched; the read/BOM logic simply moved inside the plan function. All 25 pre-existing tests
in the file still pass, including the six BOM cases and the six loud-failure cases.

### Docs

`RELEASING.md` now states the all-or-nothing property next to the loud-failure paragraph, so the runbook
says the tree is clean after an abort — which is now true and previously was not.

### Gates (all local runs on Windows PowerShell 5.1 / Node)

- `npx vitest run` (full): **324 files / 4317 tests passed**, 0 failed.
- `npm run check`: **0 errors, 0 warnings**.
- `src/lib/releaseVersionBump.test.ts` alone: **31 passed** (was 25; +6).
- No Rust touched, so no cargo gate applies.

### Not verified

- **PowerShell 7 locally** — impossible here, there is no `pwsh` on this machine and installing one is
  exactly the machine-global change that made CPE-1841's numbers unreproducible. CI's ubuntu leg carries
  it; the suite reds loudly rather than skipping if a runner ever loses its host.
- **The partial-write I/O path** — the `catch` that reports `PARTIAL BUMP` needs a `WriteAllText` to fail
  after validation (locked / read-only / full disk). Not staged; it is reported-correctly-by-construction,
  not tested.
- **The git half** of the script (`add`/`commit`/`tag`/`push`) is untouched and never exercised on a real
  repo. No release was cut.

---

## Work Log — round 2 (CI reds on the ubuntu/`pwsh` leg — the assertion, not the fix)

PR #991, head `8633340b`. **All 18 checks green** (1 `skipping` by design: GUI smoke on windows). CI
settled 2026-08-22 04:26 local.

The first push (`8240a412`) went red on exactly one check — `Frontend — type-check and test`, the
`ubuntu-latest` leg where `pwsh` is the only host — and on exactly one assertion:
`/no manifest was written/`. **323 of 324 test files passed; the fix itself was green everywhere**,
including the byte-level "first two manifests untouched" cases, on both hosts. What differed was the
message *rendering*, and it took two rounds to pin because the two hosts break it differently:

1. **`8240a412` → red.** PowerShell renders an uncaught `throw` through its own error formatter, which
   **hard-wraps the message to the console width**. The wrap fell between "No" and "manifest" under
   `pwsh` and elsewhere under 5.1, so the phrase matched locally and not on CI. Added `flat()`:
   strip ANSI CSI escapes, collapse whitespace runs, then match.
   *Trap inside the fix:* the ESC must be matched **explicitly as `\x1b`** — a bare
   `/\[[0-9;]*[A-Za-z]/` eats the literal `[p` out of `[package]`, which is a phrase these same
   assertions check for. And the escape goes in as the two-character sequence: a first attempt put a
   **raw 0x1B control byte** into the source file (the Bash tool's heredoc transport interpreted it).
   Caught by a byte scan before commit; the file ships with `rawESC=0`.
2. **`adf139be` → still red, and the captured output said why.** PowerShell 7's **ConciseView prefixes
   every continuation line of a wrapped error with a `|` gutter**, so the wrap does not merely insert
   whitespace — it inserts `"| "` mid-phrase: `... found 0. No | manifest was written -- ...`. Windows
   PowerShell 5.1 emits **no gutter at all**, which is why 5.1 stayed green through both rounds and no
   amount of local iteration would have found it. `flat()` now strips the line-leading gutter before
   collapsing whitespace, verified against the exact byte shape CI captured rather than a guess at it.
3. **`8633340b` → green**, `Frontend — type-check and test` included.

Also added `expect(out).not.toMatch(/refusing to write/i)`, which the earlier version only implied: the
retired wording is now asserted absent, not merely the new wording present.

**Worth carrying forward:** any assertion on the *text* of a PowerShell error must normalise first.
The message bytes are identical on both hosts; the formatter's wrapping and gutter are not, and CI is
the only place `pwsh` exists for this repo. `flat()` in `src/lib/releaseVersionBump.test.ts` is the
reusable form.

### Round-2 gates (local, Windows PowerShell 5.1)

- `src/lib/releaseVersionBump.test.ts` + `src/lib/mojibakeGuard.test.ts`: **93 passed**.
- `npm run check`: **0 errors, 0 warnings**.
- Test file after every edit: `loneLF=0` (CRLF intact), `rawESC=0`, no BOM, trailing `\r\n` intact.
- `scripts/release.ps1` unchanged since round 1 — rounds 2 and 3 touched only the test file.

---

## Work Log — round 4 (Reviewer APPROVED; one defect fixed, three comments corrected)

The independent Reviewer approved PR #991 and recommended merge — reproducing the md5 table to the
digit, attacking atomicity from four angles I did not try (first fails / second fails / two at once /
success), and checking CPE-1841's BOM guarantee **positively** by prefixing `EF BB BF` onto all three
manifests rather than only asserting none is added. It found one defect, which is fixed here.

### The defect: my own ticket's defect, inverted

Make the **first** write fail (read-only `package.json`). Nothing lands, the tree is clean — and the
message said:

```
PARTIAL BUMP. Already written at v9.9.9: none. Revert those files before retrying
```

"Already written: none" plus "revert those files" is a **run-untrue message telling an operator to undo
nothing** — exactly the class this ticket exists to delete, merely pointing the other way. Benign today,
which is why it did not block, and wrong for precisely the reason the ticket was filed.

Fixed at the write loop's `catch`: branch on `$written.Count -eq 0` and say

```
release.ps1: failed writing <path>: <exception> -- No manifest was written; the working tree is
unchanged and there is nothing to revert. No commit, tag or push happened.
```

The `PARTIAL BUMP` wording now only appears when a partial bump actually happened.

### The partial-bump path is tested now, not "correct by construction"

I had recorded it as untested. The Reviewer tested it in three lines, so there was no excuse. `runBump`
gained `readOnly?: "pkg" | "conf" | "cargo"`: chmod `0444` **after** staging, so the plan phase still
reads the file and the write phase fails on it. The `finally` chmods **back to 0666 before `rmSync`** —
on Windows a read-only file survives a recursive delete and cleanup throws, which would red whatever ran
next rather than this test.

Both branches covered:

- **third write fails** (`readOnly: "cargo"`) — `PARTIAL BUMP`, names `package.json` and
  `tauri.conf.json`, and the report is verified TRUE: those two are at the new version, Cargo.toml is
  byte-identical.
- **first write fails** (`readOnly: "pkg"`) — says `No manifest was written`, and asserts the absence of
  `PARTIAL BUMP`, of `Revert those files`, and of `Already written ... none`; all three manifests
  byte-identical.

Both assert with a message naming the root case, since `chmod 0444` does not block a write when running
as root.

**Red-proof by actual reversion:** put the old `$landed = if (...) { "none" } ...` single-message form
back and re-ran — **exactly 1 test failed**, "does NOT say 'revert those files' when the FIRST write
fails and nothing landed", and nothing else. Restored and md5-verified byte-exact
(`99982cb7967496f1ff3ce77ca7ff9435`).

### Three comment corrections — all were inaccurate, none of the code was

1. **The hashtable comment was wrong, though the choice is right.** It claimed the hashtable is used "so
   PowerShell emits it as ONE object here and the caller's array subexpression sees one element per
   manifest". Measured under 5.1: `@( pscustomobject; pscustomobject; pscustomobject )` gives `Count=3`
   too — identical. The hashtable is load-bearing for a different reason: `Write-ManifestVersionPlan`'s
   parameter is typed `[hashtable]$Plan`, and a `[pscustomobject]` fails argument transformation there.
   The comment now names **that**. A maintainer who checked the stated claim, found it false, and
   "corrected" it could have removed the real constraint. The Reviewer's edge cases are recorded with it:
   `@( $null; ht; ht )` → `Count=3` with `element[0]` null (a null plan does not collapse the array);
   `@( ht )` → `Count=1` (a future single-manifest variant is safe); and the stray-output hazard **is**
   real — a function emitting two objects gives `Count=4` and shifts every plan — with the audit result
   noted, that every expression in the plan function is assigned or inside an `if()`.
2. **`flat()` does destroy meaning in one case**, and round 2's Work Log offered it as "the reusable
   form". `/^[ \t]*\|[ \t]?/gm` eats any *legitimate* line-leading `|`: a markdown table
   `"| package.json | 1d81 |\n| conf | 65cb |"` comes back as `" package.json | 1d81 | conf | 65cb |"`,
   row boundary silently gone. Nothing asserted in this file depends on a leading `|`, so it is safe
   here. Now said in the code, so the next reuse is not surprised. **It is not a general-purpose
   normaliser.**
3. **The `catch` comment said "everything validated, so this is I/O".** It also wraps **parameter
   binding** — the Reviewer proved it by swapping in a `pscustomobject` and getting `Cannot process
   argument transformation on parameter 'Plan'` reported through this catch. Reworded to report the
   exception rather than assert the disk was at fault. Also added, in the plan-phase header: a
   **directory** in place of `Cargo.toml` is caught in the **plan** phase (`ReadAllBytes` throws), so
   atomicity holds and the tree stays pristine — but that throw is a raw .NET exception, with no
   `release.ps1:` prefix and no statement about tree state. The guarantee is real there; only the wording
   is not ours.

### A hazard recorded, not fixed

`pwsh` echoes the offending source line, and `release.ps1`'s throw literally contains the phrase
`No manifest was written -- every `. **If that echo were ever complete, the `/no manifest was written/i`
assertion would pass off the SCRIPT'S OWN SOURCE TEXT rather than the rendered message.** In the CI
capture the echo truncates with an ellipsis before the phrase, and 5.1 truncates too, so the assertion is
honest on both hosts today — but it is one console width or one string reflow away from being
self-satisfying. Said so in the code, next to the assertion: **the byte-level assertions in that block
are what carry the real weight; the wording match is corroboration, not proof.**

### What the Reviewer verified that stood up

- **The companion arm works.** It built the fix that arm exists to catch — patched
  `Write-ManifestVersionPlan` to write nothing at all — and got all five atomicity tests **green** with
  the companion arm **red** (plus 7 CPE-1841 tests red). A "write nothing, ever" fix genuinely would
  satisfy every atomicity assertion, and that arm genuinely stops it.
- **The CI diagnosis, confirmed from the primary log**, not taken on trust: the literal bytes
  `... found 0. No | manifest was written -- every manifest is validated before any is | written, so ...`,
  and the same message captured locally under 5.1 wrapping with **no gutter**. Both halves of "5.1 could
  never reproduce it" directly confirmed. It also ran `flat()` against `[package]`,
  `[dependencies.somepkg]`, `[lib]` and `[profile.release]` and confirmed the naive pattern destroys all
  four while the `\x1b` anchor does not.
- **Host provenance re-checked independently:** no `pwsh`; `~/.dotnet/tools/` holds only `dotnet-dump`
  and `dotnet-ildasm`; `powershell` is 5.1.26100.9168.
- **Shipped byte scan:** `release.ps1` `rawESC=0`, no BOM, `loneLF=0`; the test file `rawESC=0` and
  **zero** non-ASCII bytes; `release.ps1`'s 6 non-ASCII bytes are two pre-existing em dashes, unchanged.

### Round-4 gates (local, Windows PowerShell 5.1 — still no `pwsh` on this machine)

- `npx vitest run` (full): **324 files / 4319 tests passed**, 0 failed (was 4317; +2 partial-bump tests).
- `npm run check`: **0 errors, 0 warnings**.
- `src/lib/releaseVersionBump.test.ts` alone: **33 passed** (was 31).
- Real manifests re-measured after the script edit, on throwaway copies in a scratch repo inside the
  worktree: `1  1` for each of the three, `loneLF=0`, no BOM, trailing CRLF intact.
- Both edited files after every edit: `loneLF=0`, `rawESC=0`, no BOM, trailing `\r\n` intact; the test
  file carries zero non-ASCII bytes.
