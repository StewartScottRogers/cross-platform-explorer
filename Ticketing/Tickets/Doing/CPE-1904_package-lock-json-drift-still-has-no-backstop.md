---
id: CPE-1904
title: package-lock.json version drift still has no build-time backstop — the exact incident CLAUDE.md records is still open
type: bug
priority: Medium
status: Doing
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
bump plan. The advice cannot rot into folklore while this file stays green.

**Docs updated because they now say something false.** CLAUDE.md's "Versioning — keep five files in
sync" claimed "nothing fails when they drift"; RELEASING.md claimed "neither build passes `--locked`".
Both now describe the two mechanisms and why npm needed a different one.

**Verification:** `npm run check` 0 errors / 0 warnings. `npm test` **350 files / 5022 passed / 2
skipped** — delta **+1 file, +19 tests** against the 349 / 5003 / 2 baseline measured at the start of
this ticket. No Rust touched, so no clippy run was required.
