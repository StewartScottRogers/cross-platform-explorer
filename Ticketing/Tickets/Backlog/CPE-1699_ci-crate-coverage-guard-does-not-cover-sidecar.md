---
id: CPE-1699
title: The new CI crate-coverage guard checks crates/* but not sidecar/*, so a new sidecar can still go unwired
type: task
priority: Low
status: Backlog
tags: ready
estimate: XS
created: 2026-08-12
closed:
---

## Problem

CPE-1690 (PR #871) fixed `crates/mdns` being unverified in CI, and — better than the ticket asked —
added a **self-checking guard step** to `.github/workflows/ci.yml` that reads the workflow back and
fails if any `crates/*` directory has no matching `working-directory:` step. That guard is real: the
independent reviewer broke it two ways the author had not tried (a brand-new uncovered crate, and a
glob that matches nothing) and it redded loudly both times, with no `continue-on-error` to soften it.

The gap: the guard covers **`crates/*` only**. The PR body documents a *manual* audit that also
covered `sidecar/*` (5 crates: `agent-board`, `ai-console`, `contract`, `host`, `repos`) and found
them all wired — but a manual audit is a snapshot, and the manual half is the half that rots. A new
`sidecar/*` crate added next month can still go silently unverified, which is precisely the hole
CPE-1690 was filed to close one directory at a time.

This is not a defect in CPE-1690 — its ticket asked only for `crates/`, and it delivered more than
that. It is the obvious next notch on the same ratchet.

## Scope

`.github/workflows/ci.yml` — the crate-coverage guard step.

## Acceptance criteria

- [ ] The guard covers `sidecar/*` as well as `crates/*`, or explains in a comment why a sidecar crate
      is verified by a different mechanism and therefore does not need naming.
- [ ] **Prove it reds.** Create a throwaway `sidecar/zzz-probe/` with a minimal `Cargo.toml`, run the
      guard, and paste the real failure output. Remove the probe. The reviewer's technique on
      CPE-1690 is the standard to match: break it in a way the author did not try.
- [ ] The failure message **names the offending directory** so a maintainer can act on it without
      reading the script. Judge the wording — a guard that reds without saying which directory is a
      poor guard.
- [ ] Handle the glob-matches-nothing case deliberately. Today, with no `nullglob`, an unexpanded
      `crates/*/` happens to fail loudly rather than pass vacuously — correct outcome, confusing
      message, and reached by accident rather than design. Make it intentional and say so.
- [ ] Confirm the enumeration is still right at the time you do the work: 11 `crates/*` and 5
      `sidecar/*` as of 2026-08-12. State the scope of your check.

## Notes

Filed by the Foreman from the PR #871 review, 2026-08-12.

Related: **CPE-1690** (the guard this extends), **CPE-1694** (the same shape one level out — `gui-smoke`'s
own unit tests do not gate CI at all).
