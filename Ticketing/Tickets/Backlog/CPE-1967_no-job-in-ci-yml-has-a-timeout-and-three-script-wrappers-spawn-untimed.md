---
id: CPE-1967
title: **no job in `ci.yml` carries a `timeout-minutes` at all**, and three script wrappers spawn external tools untimed — everything sits under the 6-hour Actions default
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Found by PR #1078's Reviewer while auditing `ci-poll.mjs`, and **widened by it**: the PR's author
flagged two jobs as lacking a timeout; the Reviewer checked all ten and found **none of them has one.**

- **`ci.yml`: 0 of 10 jobs carry a job-level `timeout-minutes`.** Only individual *steps* have caps, so
  a hung job runs to the **6-hour GitHub Actions default**.
- **`gui-smoke.yml` does cap its jobs** (15 / 30 / 15 min) — so the practice exists in this repo and
  the main CI workflow is the one that skipped it.
- **`scripts/audit-npm-projects.mjs`** spawns `npm audit` with **no timeout**.
- **`scripts/sidebar-drop-stack-overlap/check.mjs:70-76`** — its CDP `send()` has **no per-call
  timeout**, unlike `layout-guard/engine.mjs`, which has `CDP_CALL_TIMEOUT_MS`.

## Why it is worth a ticket rather than a note

This sprint spent **over an hour** unable to tell a *slow* `Server crates (windows-latest)` job from a
*hung* one, with two approved PRs blocked behind it, and settled it only by comparing start timestamps
against the same job on a sibling PR by hand. A job timeout would have answered that question in the
runner rather than in the Foreman's head.

It is also the **fail-open family** in its purest form, one layer out from where the sprint has been
fighting it: a process that never finishes never reports, and "never reported" is the one state no
verdict can classify. CPE-1906 fixed the poller's side (a hung `gh` no longer blows through the
advertised budget); this is the runner's side of the same problem.

**Deliberately not fixed in PR #1078** — that PR is about `ci-poll.mjs`, and adding workflow timeouts
there would have been unreviewable scope creep.

## Acceptance criteria

- [ ] **Enumerate, don't recall** (CPE-1932): derive the job list at run time (`git ls-files
      '.github/workflows/*.yml'`) and report every job with its current `timeout-minutes`, rather than
      fixing the ten someone remembered. There are 8 workflow files.
- [ ] **Pick each timeout from measured duration, not a round number.** Job durations are available
      from `gh api`; the whole-run median here is **58.9 min**, and per-job spreads are wide —
      `Server crates (windows-latest)` legitimately runs ~60 min while `Frontend` runs ~19 min, so one
      shared constant is wrong. **Record the measurement beside each value**, or the next person will
      not know whether a timeout that fires means "hung" or "we guessed low".
- [ ] Add per-call timeouts to `audit-npm-projects.mjs`'s spawn and `check.mjs`'s CDP `send()`. Match
      `layout-guard/engine.mjs`'s existing `CDP_CALL_TIMEOUT_MS` shape rather than inventing a second.
- [ ] **A timeout must fail loudly and distinguishably.** A job killed at its cap must not read as any
      other kind of failure, and must not read as a skip — `ci-poll.mjs` now has an exit-4 "did not
      run" verdict, and a timed-out job is a *third* thing again. Check what the rollup reports for a
      cancelled-by-timeout job and make sure the poller classifies it correctly.
- [ ] **Consider a guard** asserting every job in every workflow declares a `timeout-minutes`. This
      repo's pattern is that an enumerated invariant gets a test (`ciVerdict.test.ts`,
      `catalogPublishLoudFailure.test.ts`), and "someone adds a job without a cap" is exactly the drift
      that recreates this. If you add one, it must **derive** the job list from the parsed YAML, not a
      hard-coded list — and anchor on the parsed document, not on comment text (CPE-1933).
- [ ] **Widen the sweep past `*.mjs`.** PR #1078's audit covered `scripts/*.mjs` only;
      `scripts/release.ps1`, `new-sample-sandbox.{sh,ps1}` and `gen_samples.py` also wrap external
      tools and were never checked. Report a verdict per script.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1078's Reviewer, which measured all ten `ci.yml` jobs
rather than accepting the two the PR named.

Related: **CPE-1906** (the poller's side of the same problem — a hung `gh` crossing the advertised
budget, PR #1078), **CPE-1956** (`ci.yml`'s silent-skip gate, PR #1074), **CPE-1932** (enumerate,
don't recall), **CPE-1171** (the gui-smoke harness, which already caps its jobs).
