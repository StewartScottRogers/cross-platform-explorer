---
id: CPE-1904
title: package-lock.json version drift still has no build-time backstop — the exact incident CLAUDE.md records is still open
type: bug
priority: Medium
status: Backlog
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
