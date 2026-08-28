---
id: CPE-1904
title: package-lock.json version drift still has no build-time backstop — the exact incident CLAUDE.md records is still open
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1865 gave the Rust lockfiles a real backstop: `--locked` now refuses a stale `Cargo.lock`, and
CPE-1855's UAT confirmed it fires on **both** of this repo's independent Rust lockfiles —
`crates/*/Cargo.lock` and `src-tauri/Cargo.lock` — exit 101 with Cargo's own clear message.

**The npm side is still unguarded, and the npm side is the one that actually bit.** CLAUDE.md's
versioning section records the real incident: `package-lock.json` had been **three releases behind**
(`0.57.64` vs `0.57.67`), observed 2026-08-20. It names items 4 and 5 as "the ones that get missed",
and item 4 is `package-lock.json` — in **two** places, the top-level `version` and
`packages[""].version`.

Verified directly by CPE-1855's UAT, not inferred: bump `package.json`'s `version`, leave
`package-lock.json` untouched, run `npm ci` → **exit 0, "up to date"**. No failure, no repair, no
signal of any kind.

CPE-1865's own Work Log discloses this plainly — *"nothing at build time gives `package-lock.json`'s
version fields a backstop... only the release script's all-five guard"* — so this is an honest partial
fix rather than an overclaim. It is filed as its own ticket because the disclosed half is the half that
caused the recorded incident.

## Why the release script is not enough

`scripts/release.ps1`'s all-five check runs when someone cuts a release **through it**. The failure
mode CLAUDE.md describes does not surface there — it surfaces as *"a dirty working tree the moment
anyone runs `npm install` or a local `cargo build`"*, which "reads as unrelated noise and gets
committed by accident or discarded along with real work". A guard that only fires at release time
cannot catch drift that is introduced, and then laundered, between releases.

## Acceptance criteria

- [ ] Fail the build — or a CI job that runs on every push and PR — when `package-lock.json`'s
      `version` or `packages[""].version` disagrees with `package.json`'s. Both fields; the second is
      the one that gets missed because it does not look like a version field at a glance.
- [ ] Red-proof it: bump `package.json` alone and confirm red naming both the file and which of the two
      fields drifted; bump only the top-level lock field and confirm it still reds on
      `packages[""].version`; sync all three and confirm green.
- [ ] Confirm the false-alarm case stays quiet: adding or removing a dependency (which legitimately
      rewrites the lockfile without touching versions) must not trip it.
- [ ] Say plainly in the failure message what to run to fix it. Cargo's `--locked` message is
      serviceable but not friendly — CPE-1855's UAT noted a newcomer may stall on "use `--offline`
      instead". Do better here: name the command.
- [ ] While in this area, check whether `npm ci` alone would have caught the *original* incident had the
      lockfile been genuinely inconsistent rather than merely version-stale. Record the answer — it
      decides whether this needs its own check or a stricter invocation of an existing one.

## Notes

Filed 2026-08-26 from CPE-1855/CPE-1865's independent UAT, which tested the npm half specifically
because CLAUDE.md names it as the historical failure, and found it open.

Related: **CPE-1865** (the Rust half, honest about this gap), **CPE-1855** (the MSRV floor it shipped
with), **CPE-1853** (`cpe-1853-lockfile-version-sync`, an in-flight branch in the same area — check
whether it already covers this before starting).

Note the five-files rule this belongs to, from CLAUDE.md: `package.json`, `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`, `package-lock.json` (two places), `src-tauri/Cargo.lock`. CPE-1865 closed
the fifth. This closes the fourth. The first three already fail loudly when they drift.

## Work Log

### 2026-08-27 — implemented (branch `cpe-1904-package-lock-version-backstop`)

**First: reproduced the defect, on `b5658d93`, before writing anything.** With both of
`package-lock.json`'s version fields deliberately drifted (root `"version"` → 0.57.66,
`packages[""]."version"` → 0.57.64, against a `package.json` of 0.57.69):

| command | exit | output |
|---|---|---|
| `npm ci` | **0** | "added 191 packages, and audited 192 packages" — no warning |
| `npm test` | **0** | 349 files / 5003 passed / 2 skipped |
| `npm run check` | **0** | "svelte-check found 0 errors and 0 warnings" |
| `npm install --package-lock-only` | **0** | **silently repaired both fields** |

That last row is the ticket's whole point restated as a measurement: the command that reveals the drift
is the command that destroys the evidence of it, leaving a dirty working tree with no message attached.

`src-tauri/Cargo.lock` (item 5) was measured separately and is **already backstopped**: with its
`cross-platform-explorer` entry at 0.57.66, `cargo metadata --locked` exits **101**. CPE-1865 and
CPE-1932 did their job. It is covered here anyway — the cargo failure needs a toolchain and an hour-long
matrix to reach, its message names neither the field nor a fix, and a guard that enumerates five of six
places is the same defect with extra steps.

**Also checked CPE-1853's branch first, as the ticket asked.** Its `release.ps1` all-five bump has
already landed on `main`, so there was no overlap — but it is a release-time guard and the incident was
drift introduced *between* releases.

**The last AC, answered by measurement rather than assertion.** Would `npm ci` have caught the original
incident had the lockfile been genuinely inconsistent? **Yes, and loudly** — adding `left-pad` to
`package.json` without touching the lockfile gives exit **1**, `EUSAGE`, "Missing: left-pad@1.3.0 from
lock file". `npm ci` enforces the dependency graph; it just never looks at the `version` fields. The two
are orthogonal, and the recorded incident was a graph-consistent lockfile with a three-release-old
version. No npm flag closes that. It needed its own check.

**Where it went, and why.** `src/lib/appVersionSync.test.ts` — a vitest guard, so it runs in ci.yml's
`frontend` job on every push and PR **and** on every local `npm test`. The local half is the point: the
drift is introduced and then laundered locally, before anything is pushed.

- *A `--locked`-style build failure* was the obvious answer and is not on offer. `npm ci` **is** npm's
  `--locked`, it is already what CI runs, and the table above is it exiting 0. It would have bought the
  earliest possible failure on every consumer of the lockfile; npm simply does not treat those fields as
  a constraint. (For `Cargo.lock` it *is* on offer and already landed.)
- *A dedicated node-only CI job* (like `ratchet-guard`/`npm-audit-sweep`) would have bought its own red X
  in the checks list and independence from `npm ci` succeeding first. The second is moot — version drift
  provably does not break `npm ci` — and the first costs the local run. The enumeration and the verdict
  are exported, so promoting it later is a five-line change.

**Derived, not recalled (CPE-1932).** `git ls-files` supplies the candidates; each is keyed on the app's
package **identity**, seeded from the npm project root. `gui-smoke/package.json` (`cpe-gui-smoke`),
`gui-smoke/package-lock.json` and **16 of the 17** tracked `Cargo.lock`s are excluded by what they say
about themselves, not by a path the guard knows. `tauri.conf.json` is the one family that cannot be
identity-keyed (Tauri config has no package `name`) — matched on filename, and said so at the site
rather than papered over. Two-sided: `MIN_VERSION_PLACES` refuses a near-empty sweep,
`KNOWN_VERSION_PLACES` is the human tripwire, the same shape `npmProjects.test.ts` uses.

**Red-proofed each place independently, in the real working tree**, restoring and `cmp`-verifying
byte-identical after each:

| place drifted to 0.57.66 | result |
|---|---|
| `package.json` `"version"` | 1 failed / 18 passed — names all **five** others |
| `package-lock.json` root `"version"` | 1 failed / 18 passed — names it alone |
| `package-lock.json` `packages[""]."version"` | 1 failed / 18 passed — names it alone |
| `src-tauri/Cargo.toml` `[package]` version | 1 failed / 18 passed — names it alone |
| `src-tauri/Cargo.lock` `[[package]]` entry | 1 failed / 18 passed — names it alone |
| `src-tauri/tauri.conf.json` `"version"` | 1 failed / 18 passed — names it alone |

Every message names the file, the field, **both** values, and the command to run.

Two things in that table were earned, and both are recorded at the site:

1. **"1 failed", not "8 failed".** The first round produced eight — the fixture-based tests copied the
   drifted tree and faithfully reproduced the sabotage. `syncedFixture` normalises the fixture first, so
   a real drift gives one clear failure instead of noise to read past.
2. **The `package.json` row nearly read as a pass.** The harness aimed its `sed` at line 3; the version
   is on line 4. Silent no-op, green run, looked exactly like the guard failing to fire — a fail-open
   red-proof inside the ticket about fail-open guards. The harness now verifies its own sabotage landed.

**Fail-closed, measured on the real tree** (not only on fixtures): truncating `src-tauri/Cargo.lock`
mid-entry gives `Error: src-tauri/Cargo.lock: did not parse as TOML (Line 1609: expected end of line)`,
`Tests no tests`, vitest exit **1**. Six more fail-closed cases are permanent tests: invalid JSON, a
missing `packages[""]`, a present-but-unreadable `tauri.conf.json` version, a missing `Cargo.toml`
version key, a vanished file, and an unreadable app identity.

**False-alarm case stays quiet:** adding a dependency entry plus a root dependency edge to the lockfile
(a large diff touching neither version field) leaves the verdict empty.

**CPE-1933:** the failure message tells you to run `scripts/release.ps1 -BumpOnly`, so a test reads that
script and asserts it declares `[switch] $BumpOnly` and that every file the guard checks appears in its
bump plan. The advice cannot rot into folklore while this file stays green. *(Round 2 found this
particular assertion was itself shadowed and matched prose — see the round-2 entry below; it was
deleted and the derivation credited to `releaseVersionBump.test.ts`, which really does execute the
script.)*

**Docs updated because they now say something false.** CLAUDE.md's "Versioning — keep five files in
sync" claimed "nothing fails when they drift"; RELEASING.md claimed "neither build passes `--locked`".
Both now describe the two mechanisms and why npm needed a different one.

**Verification:** `npm run check` 0 errors / 0 warnings. `npm test` **350 files / 5022 passed / 2
skipped** — delta **+1 file, +19 tests** against the 349 / 5003 / 2 baseline measured at the start of
this ticket. No Rust touched, so no clippy run was required.

---

### Round 2 (2026-08-27) — review APPROVE, two non-blocking findings, both closed

**F1 — the CPE-1933 test was itself a CPE-1929 shadowed guard, and it matched prose. Deleted.**

`it("names a fix command that actually exists")` asserted `expect(script).toContain(basename(file))`
against **raw `release.ps1` text, comments included**. That script's header comments name all five
files repeatedly, so **prose alone satisfied it** — CLAUDE.md's rule 2 ("anchor on code, never on
prose"; a whole-line-comment filter is not enough) failing inside the test written to honour rule 2.

Sabotage re-run here, the Reviewer's exactly: drop `src-tauri/tauri.conf.json` from release.ps1's
`$plans` (line 453), from its `Write-Host "Bumped version to …"` summary, and from `Invoke-Git add`, so
the script genuinely stops bumping *and* staging it. `release.ps1` restored and `cmp`-verified
byte-identical (md5 `3804ce4f…`) after each run.

| suite | before the fix | after the fix |
|---|---|---|
| `appVersionSync.test.ts` | **19 passed** (green while the script was broken) | **19 passed** — but it no longer claims to check the plan |
| `releaseVersionBump.test.ts` | **10 failed / 55 passed** | unchanged |

The ten include *"release.ps1 plans exactly the files CLAUDE.md's five-files-in-sync list names"* and
*"stages all five in the release commit"*. Green-while-broken sitting next to a red that caught it:
safe, unverifiable, and reading as coverage — the exact pair CPE-1929 describes.

**Chose delete over repair (option a), for three reasons.**

1. `releaseVersionBump.test.ts` does not read the script, it **runs** it: `runBump` copies the real
   `scripts/release.ps1` into a fixture tree and spawns it with `-BumpOnly`, so a renamed switch is a
   PowerShell parameter-binding failure rather than a passing regex, and `$plans` is joined by **set
   equality** across three sources (CLAUDE.md's numbered list, its own `FILE_PATHS`, and the argv of
   every `New-ManifestVersionPlan -Path (Join-Path $repo "…")` call), plus the `Invoke-Git add` line.
2. A repaired version (comments stripped, scoped to `$plans`, switch anchored `\$BumpOnly\s*\)`) would
   close the prose hole and leave the **shadowing** untouched — it would still assert a strict subset of
   what the other file already asserts by equality. CPE-1929's answer to a shadowed guard is
   reorder-or-delete; there is no reorder available across files, so delete.
3. It would grow a second PowerShell scanner inside a file about JSON and TOML version fields —
   CPE-1950's "remove the duplication where it is removable" points the other way.

**What was kept, because it is the one part that is genuinely NOT shadowed:** the script path is parsed
**out of `FIX_ALL`** (not retyped, or it would be a literal checking itself) and checked to exist.
`releaseVersionBump.test.ts` reads its own `join(ROOT, "scripts", "release.ps1")` constant and never
sees `FIX_ALL` (grep: 0 references), so a script moved and updated there but not here leaves the fix
advice pointing at nothing and only this assertion notices. Red-proofed by pointing `FIX_ALL` at
`scripts/moved/release.ps1`: **1 failed / 18 passed**, `scripts/moved/release.ps1, which FIX_ALL tells
a drifting developer to run, does not exist`. Test file then restored `cmp`-identical. The full
measurement, both columns of the table, and the delete-not-repair argument are written **at the site**,
per CPE-1933 rule 3.

**F2 — the disclosure named the wrong cost. Fixed at the site and in the message.**

The header framed hard-parsing 34 unrelated Rust manifests as a **speed** cost (55 ms). Reproduced the
real cost here — appending

```toml
[package.metadata.cpe1904]
note = """
multi-line
"""
```

to `crates/mdns/Cargo.toml` (restored afterwards, `cmp`-identical, md5 `10904adc…`) gives

```
Error: crates/mdns/Cargo.toml: did not parse as TOML (Line 25: multi-line strings ("""…""") are not supported by this preview)
Tests  no tests        exit 1
```

The whole file fails to collect, and a dependency bump in an unrelated crate is answered with a message
about a **preview parser's scope**. So the cost is **coupling**, not speed. Polarity unchanged — a
manifest this guard cannot read is not one it may skip. Two changes: the header now names that failure
mode with the reproduction inline, and `readToml`'s throw now points at **`src/lib/preview/toml.ts`**
and its deliberate gaps, says the fix belongs there if the file is valid TOML, and says why it throws
instead of skipping. The existing fail-closed case still matches
(`/src-tauri\/Cargo\.lock: did not parse as TOML/`).

**Not reopened, per review:** the `src-tauri/Cargo.lock` coverage stays despite `cargo metadata
--locked` exiting 101 — cargo names the file but never the field or the values, its only "help" is the
`--offline` trap that regenerates the lockfile rather than reporting the stale version, and reaching it
costs a Rust toolchain plus a preflight run. Here it is ~0 ms of the 400 ms, and covering five of six
places would have been the exact CPE-1932 enumeration defect.

**Verification (round 2):** rebased on `origin/main` `161930fc`. `npm run check` **0 errors / 0
warnings**. `npm test` **350 files / 5022 passed / 2 skipped** — delta **+1 file, +19 tests**,
unchanged by this round (the deleted assertions lived inside a test that is still one test). No Rust
touched.

## Closed 2026-08-27 — what the gauntlet actually proved

Merged as PR #1082, **fully green (25/25)**, after two rounds.

**The sharpest single measurement of the shift.** With both `package-lock.json` version fields drifted
(root → `0.57.66`, `packages[""]` → `0.57.64`, against a `package.json` of `0.57.69`):

| command | exit | output |
|---|---|---|
| `npm ci` | **0** | "added 191 packages" — no warning, lockfile left drifted |
| `npm test` | **0** | 349 files / 5003 passed |
| `npm run check` | **0** | 0 errors, 0 warnings |
| `npm install --package-lock-only` | **0** | **silently repaired both fields** |

**The command that reveals the drift is the one that destroys the evidence of it.** Its Reviewer
reproduced all four rows and added the detail that makes it worse in person: `git diff --stat` goes
**empty**. The only trace the incident ever left — a dirty working tree that reads as unrelated noise —
is gone, with no message and no exit code.

**A `--locked`-style failure was not on offer, and that was measured, not assumed.** `npm ci` **is**
npm's `--locked`, and it exits **0** on version drift — while correctly exiting **1** with `EUSAGE`
*"Missing: left-pad@1.3.0 from lock file"* on a genuinely inconsistent lockfile. It enforces the
dependency **graph** and never reads the version fields; the recorded incident was a graph-perfect
lockfile **three releases stale**. Orthogonal, so this needed its own check.

**Placement argued, not defaulted:** vitest, so it runs in CI's `frontend` job **and on every local
`npm test`** — because **the drift is introduced and laundered locally**, and a CI-only job never sees
the `npm install --package-lock-only` step that destroys the evidence. Its Reviewer verified `frontend`
carries no `needs:` and no path filter rather than taking that on faith.

**Its own red-proof harness had a fail-open, inside the ticket about fail-open guards.** The
`package.json` case **nearly read as a pass** because the harness's `sed` aimed at line 3 when the
version sits on line 4 — a sabotage that changes nothing looks exactly like a guard that catches
nothing. Fixed so the harness cannot pass while drifting nothing; the Reviewer reproduced the fix by
forcing a nonexistent occurrence and watching **all six** red with *"fixture drift of <file> (<field>)
changed nothing."* A second harness defect (fixtures reproducing the sabotage) was reproduced **to the
digit** at 8 failures.

**Fail-closed, measured on three shapes**, including the one that produces a *plausible* answer: a
well-formed lockfile with the package entry **absent** reports *"Only 5 version place(s) were found, and
this guard refuses to pass a verdict on fewer than 6"* — **not** "5 of 5 agree". `MIN_VERSION_PLACES`
catches it independently of the `KNOWN_VERSION_PLACES` tripwire, so the derived list is the one that
fails, not the remembered one.

**Round 2 deleted a shadowed guard rather than repairing it.** `it("names a fix command that actually
exists")` asserted a filename appears in `release.ps1` — against **raw text, comments included**, whose
header names all five files, so **prose alone satisfied it**. Deleting `tauri.conf.json` from the
script's `$plans`, its summary and its `Invoke-Git add` left this test **19/19 green** while
`releaseVersionBump.test.ts` reddened **hard** (10 failures). The argument for delete over repair is
worth keeping: that sibling test does not *read* the script — it **spawns** it with `-BumpOnly` (so a
renamed switch is a parameter-binding failure, not a passing regex) and joins three sources by **set
equality**. The deleted assertion was weaker on all three axes at once, and ***a repaired subset of an
equality assertion is still a subset.*** It then **red-proofed what it kept**.

**Item 5 was already backstopped and covered anyway, with a reason:** `src-tauri/Cargo.lock` at a
drifted version makes `cargo metadata --locked` exit **101** — but that message names the **file** and
never the **field** or the values, its only "help" is the `--offline` trap that regenerates the lockfile
rather than reporting staleness, and reaching it costs a Rust toolchain plus a preflight run. Five of
six places would also have been the exact CPE-1932 enumeration defect.
