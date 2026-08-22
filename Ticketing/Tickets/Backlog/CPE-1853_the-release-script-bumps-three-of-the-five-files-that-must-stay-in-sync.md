---
id: CPE-1853
title: the release script bumps three of the five files that must stay version-synchronised
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-22
closed:
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

- [ ] `package-lock.json`'s **both** version fields and `src-tauri/Cargo.lock`'s `cross-platform-explorer`
      entry are bumped by the same script, under the same exactly-one-match guard.
- [ ] `Cargo.lock` is scoped to the **right package entry**. Every other package's version in that file is a
      dependency pin — rewriting one is precisely the defect CPE-1841 existed to fix, in a file that
      contains hundreds of them. Test with a decoy package whose version matches the app's.
- [ ] `package-lock.json` needs **two** edits, not one. A test must fail if only one lands — that is the
      specific way this file goes stale.
- [ ] Red-proof each locator with the minimal realistic change, observe red, revert, record the line.
- [ ] Byte-level: preserve CRLF, the trailing newline, and BOM presence-or-absence per file, and keep the
      diff minimal. CPE-1841 measured `1 1` per file for the three it handles; state the expected numstat
      for the two new ones and hold it.
- [ ] A single guard test asserts **all five** carry the same version after a bump, so the next file added
      to the list cannot be silently forgotten.
- [ ] Say explicitly whether the build should also start passing `--locked`. That is the change that would
      make drift fail loudly on its own instead of relying on this script — out of scope to implement here,
      but the decision belongs in this ticket's record.

## Notes

Raised by the independent Reviewer during CPE-1841 as the natural home for this check, given the
loud-failure machinery that ticket introduced.

Related: CPE-1841 (the scoped locators and the exactly-one guard), CPE-1852 (the half-bumped tree that
lands in the same working directory), CPE-1834 and CPE-1842 (the encoding fixes on the same release path).
