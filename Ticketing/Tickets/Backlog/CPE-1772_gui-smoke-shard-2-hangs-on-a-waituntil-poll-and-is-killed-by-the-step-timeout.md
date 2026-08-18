---
id: CPE-1772
title: GUI smoke shard 2 hangs on a waitUntil poll and is killed by the step timeout, ~30 minutes later
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Observed three times in three days, twice on `main` itself and once on PR #923 during the batched sprint of
2026-08-17.

`GUI smoke (ubuntu-latest) shard 2` hangs in a `waitUntil` poll loop in `samples.smoke.ts`, produces no
further output, and is eventually **cancelled by the step timeout after roughly 30 minutes**. Re-running the
failed job alone (`gh run rerun <id> --failed`) passes it clean, so the underlying test is not broken —
it is non-deterministic.

Evidence:

- `main` @ `244337f4` — its last three GUI-smoke runs carry **two `cancelled` conclusions**, before PR #923
  existed. Pre-existing, not introduced by any of this sprint's work.
- PR #923 — shard 2 hung, was cancelled, passed on a targeted re-run of only the failed job. The PR's own
  diff touched one pure frontend function (`src/lib/tags.ts`) with no GUI surface, so it cannot plausibly
  have caused a GUI-smoke hang.

## Why this is High despite being "just a flake"

1. **It costs 30 minutes of a contended runner every time it fires.** During this sprint six PRs were queued
   at once and the runner pool began cancelling jobs; a leg that occupies a runner for half an hour doing
   nothing makes that materially worse for every other PR.
2. **It trains people to re-run red.** A shard that fails for no reason teaches whoever is watching to hit
   re-run without reading the log. The next time shard 2 goes red for a *real* reason, that is the reflex it
   will meet. That is the expensive failure, and it is the reason to fix this rather than tolerate it.
3. **A cancellation is indistinguishable from a genuine timeout hang** in the PR checks UI — both read as a
   red X. So a real deadlock in the app would look exactly like this and be dismissed as "the usual shard 2
   thing".

## What to do

- **Find why the poll never settles.** `waitUntil` in `samples.smoke.ts` is the site. Determine what
  condition it is waiting on and what state the app is actually in when it hangs — capture a screenshot and
  the DOM at timeout rather than only the poll's own failure. If the harness cannot currently capture that,
  making it do so is the first piece of work and is valuable regardless of this bug.
- **Distinguish "the app is wrong" from "the runner could not paint".** The workflow already has a step
  named *"Classify suite log — app defect vs runner-could-not-paint (CPE-1728)"*, which suggests this
  distinction is already modelled. Check whether the hang is being classified at all, or whether the step
  timeout kills the job before the classifier ever runs — in the observed failure the classifier itself
  exited 127 after the cancellation, which means it never got to judge.
- **Give the poll a real deadline, well under the step timeout**, so it fails as a *test failure with
  diagnostics* rather than as a runner cancellation 30 minutes later. A test that fails in 60 seconds with a
  screenshot is worth more than one that dies silently in 30 minutes.
- **Then decide whether to retry.** An automatic single retry of a genuinely flaky leg is defensible, but
  only after the above — retrying first is how a real regression gets buried.

## Acceptance criteria

- [ ] The root cause of the non-settling poll is identified and stated, not merely worked around.
- [ ] When the poll does not settle, the job fails **within a bounded time well under the step timeout**,
      and emits a screenshot plus the relevant DOM/console state.
- [ ] A hang is classified as app-defect vs runner-could-not-paint by the existing CPE-1728 classifier, and
      the classifier actually runs — verify it is not skipped by the cancellation path.
- [ ] Ten consecutive runs of shard 2 on an unchanged tree are green. State the actual pass count; do not
      infer stability from one green run.
- [ ] If an automatic retry is added, a retried failure is still visible in the run summary — a silently
      retried leg is a leg nobody knows is sick.

## Notes

Reported by the CPE-1754 worker during the batched sprint of 2026-08-17, on the second occurrence it saw,
and confirmed against `main`'s own recent run history. Related: CPE-1753 (the sharding that makes a missing
shard red), CPE-1728 (the app-defect vs runner-could-not-paint classifier), CPE-1171 (the GUI smoke harness).
