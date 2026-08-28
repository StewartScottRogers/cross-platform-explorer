---
id: CPE-1969
title: two workflow-scan enumeration gaps — `lockfileLockedGuard` hard-codes five files, and **no consumer scans `.github/workflows/scripts/*.sh` at all**
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Found by PR #1080's Reviewer while verifying CPE-1936's heredoc fixes. Both are **CPE-1932** — *enumerate,
don't recall* — and both were invisible until someone swept the directory instead of reading a list.

**Gap 1 — `src/lib/lockfileLockedGuard.test.ts`'s `WORKFLOW_FILES` is a hard-coded five-file list.**
There are **eight** workflow files. A sixth workflow that builds Tauri without `--locked` is simply not
looked at. CLAUDE.md's own rule: *"Any guard over 'all the X in this repo' derives its list at run time
(`git ls-files`, a tree walk) and fails loudly when the list comes back near-empty — a hard-coded list
of the instances someone remembered is how seventeen Cargo.lock files became two."*

**Gap 2 — nothing scans `.github/workflows/scripts/*.sh`.** Three scripts (39 / 50 / 20 lines,
measured), invoked *by* the workflows, and **no consumer reads them**: not the hang-hardening scan, not
the lockfile guard, not channel purity, not either Rust consumer. Every guard built on
`shellScriptLines` stops at the `run:` block boundary, and shell that has been *moved out* of a `run:`
into a file is out of scope by construction.

Gap 2 is the more interesting of the two, because **extracting shell into a script is normal, good
refactoring** — and here it silently removes that shell from every guard. Nobody did anything wrong;
the guards' scope just never followed the code.

## Why now

CPE-1936 (PR #1080) just fixed two `shellScriptLines` mis-parses, one of which (**N8**) had made **160
logical lines of `ffmpeg-pin-freshness.yml` invisible** to the hang-hardening scan — including an
`exit 1` and a whole error branch. Nothing was answering wrongly only because the blind window happened
to contain no `curl`, no `apt`, no `--expect-channel`, no `--locked`.

**These two gaps are the same defect one level up:** the parser now reads its input correctly, and the
input is still not everything it should be. A guard that parses perfectly over the wrong file set is
exactly as blind.

## Acceptance criteria

- [ ] **Derive both lists at run time** (`git ls-files '.github/workflows/*.yml'`,
      `git ls-files '.github/workflows/scripts/*'`) and **fail loudly when either comes back
      near-empty** — that near-empty check is the half CLAUDE.md singles out, and it is the half that
      gets left off.
- [ ] **Before changing anything, run each guard over the newly-included files and report what it
      finds.** If a sixth workflow or one of the three scripts contains an unhardened command, an
      unlocked Tauri build, or a `--expect-channel` invocation, **that is a live defect and needs its
      own ticket** — do not fold a real finding into the enumeration fix.
- [ ] **Decide what "a step" means for a standalone `.sh`.** The `step.run` consumers are built around
      YAML steps; a script file has no step. Say how you map it, and whether per-step and whole-file
      views both make sense there.
- [ ] **Check the other `shellScriptLines` consumers for the same scope gap** — `releaseHangHardening`,
      `channelPurityCoverage`, `release_workflow_wiring.rs`, `artifact_binding.rs`. The last two read
      only `release.yml` and `release-sidecar.yml`; say whether that is a deliberate scope or another
      remembered list.
- [ ] **Red-proof each**: add a sixth workflow (or a fourth script) containing exactly the thing the
      guard hunts, confirm it is now caught, and confirm the near-empty check fires when the glob
      matches nothing.
- [ ] Keep the shared oracle honest — if scanning `.sh` files needs any parser behaviour the YAML path
      does not, it belongs in `src/lib/shellScriptLines.cases.json` so **both** implementations get it.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1080's Reviewer, which swept the workflows directory
and the `scripts/` subdirectory rather than trusting either list, and flagged both as out of scope for
that PR — correctly.

Related: **CPE-1936** (the heredoc mis-parses, PR #1080 — where the 160-line blind window was
measured), **CPE-1932** (enumerate, don't recall — and the seventeen-Cargo.lock incident),
**CPE-1849** (which folded `ffmpeg-pin-freshness.yml` into `GUARDED`), **CPE-1933** (anchor on code,
never on prose — the rule `shellScriptLines` exists to serve).
