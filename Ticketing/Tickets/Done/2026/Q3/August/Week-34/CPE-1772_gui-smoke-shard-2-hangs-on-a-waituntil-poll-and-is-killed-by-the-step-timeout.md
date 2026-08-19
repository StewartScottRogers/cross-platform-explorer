---
id: CPE-1772
title: GUI smoke shard 2 hangs on a waitUntil poll and is killed by the step timeout, ~30 minutes later
type: bug
priority: High
status: Done
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

## Further evidence, same evening (2026-08-17, batched sprint)

Three GUI-smoke shard failures across three unrelated PRs within about ninety minutes, none of which
touches GUI code:

| PR | Ticket | Diff | Shard | Outcome |
|---|---|---|---|---|
| #923 | CPE-1754 | `src/lib/tags.ts`, one pure function | shard 2 | hung, cancelled at the step timeout; passed on a targeted re-run |
| #924 | CPE-1715 | `src-tauri/src/lib.rs`, name-picking probe | shard 1 | cancelled during *"Install Linux system dependencies"*, before any test ran |
| #926 | CPE-1758 | `crates/server/src/archive.rs` + docs | shard 3 | failed |

Two things this adds to the report above:

1. **It is not confined to shard 2**, so a fix aimed only at `samples.smoke.ts`'s poll will not close it.
   Shard 1's failure happened during *dependency installation* — before any test executed — which is a
   different mechanism entirely and points at runner contention or setup flakiness rather than a hanging
   assertion.
2. **Contention appears to be a factor.** All three occurred while six PRs were queued against the runner
   pool simultaneously. On #924 the real CI leg (cargo check/clippy/test across all three OSes) **succeeded
   on the same commit** while GUI smoke was cancelled — so the cancellation carried no code signal at all.

That last point is the operational cost: a red X that means nothing is indistinguishable from a red X that
means everything, and a Foreman draining a merge queue has to open each one to find out.

### Fourth occurrence, and this one takes every shard down with it

PR #933, 2026-08-18. The failing job was **"GUI smoke — build once for every shard (CPE-1753)"** — the
*shared* build, not a shard. It was cancelled during `Install tauri-driver`, after the gui-smoke unit tests
had all passed and `tauri-driver v2.0.6` had finished installing:

```
Installed package `tauri-driver v2.0.6` (executable `tauri-driver`)
##[error]The operation was canceled.
```

Then the cross-shard verdict job went red too, because its input never arrived.

That makes four in one night, at four different points: a `waitUntil` poll (#923 shard 2), dependency
installation (#924 shard 1), a shard failure (#926 shard 3), and now toolchain installation in the shared
build (#933). **None was in test code and none carried a code signal.** The shared-build case is the worst
shape: one cancellation there fails every shard at once, so a PR shows a wall of red that means nothing.

The pattern across all four is a cancellation at an arbitrary setup step under runner contention, which
argues the root cause is not `samples.smoke.ts`'s poll at all — that was just where the first one landed.
Whatever is done here should start from "why are these jobs being cancelled" rather than from the poll.

## Work Log

2026-08-19 — Root-caused the `connectionRetryTimeout` (180s) x default `connectionRetryCount` (3)
compounding in `wdio.conf.ts`: one wedged WebDriver command could silently retry for up to 12 minutes
with zero output, and it's never just one command (afterEach's `snapFailure`, the next test's setup).
Fixed with `connectionRetryCount: 0`. That removal opened a narrower race in `beforeSession` (spawns
`tauri-driver`, returned before it was listening) — closed with a bounded TCP-readiness poll instead of
restoring the retry count. See PR #935 for the full diffs.

2026-08-19 — CORRECTION to this ticket's own "What to do" section (the "the classifier itself exited 127
after the cancellation" line): pulled the raw log for the exact incident cited there (PR #923 shard 2,
job 95580439494) via `gh api .../jobs/<id>/logs`. The classify-log step (CPE-1728) **ran and succeeded**,
correctly printing `ENVIRONMENT SIGNATURE ONLY — no AssertionError... 6 known WebDriver/runner marker(s)
matched`. The actual failure was the NEXT step, Ratchet, throwing `failed to parse
wdio-shard-2-of-4-0-7.json as JSON: Unexpected end of JSON input` and exiting **1, not 127** — a truncated
results file from the job being killed by the 30-minute JOB-LEVEL cap (`gui-smoke.yml`'s `timeout-minutes`
on the shard job), not a step-level poll timeout, and not a case where the classifier "never got to
judge" — it judged, correctly, and was overruled by the job dying mid-write one step later. The exit-127
mechanism described in "What to do" (apt hang -> `npm ci` never runs -> `tsx` missing) is real and was
independently observed on PR #935's own CI during the CPE-1781 companion work, but it is a DIFFERENT
incident than the one originally cited here — corrected rather than left standing uncorrected next to a
newer, accurate account. One piece of this incident's log DOES strengthen the `connectionRetryCount`
theory: the suite step ran 03:13:43-03:25:26 (11m43s) before cancellation — close to the theorised
4 x 180s = 12-minute ceiling for a stuck command retried to exhaustion.

2026-08-19 — Acceptance criterion "ten consecutive green shard-2 runs" is explicitly **NOT MET**.
Observed count: 1 clean shard-2 pass (run 32275319086 attempt 2, 17:19-17:33 UTC) plus one earlier rerun
that was itself cancelled mid-flight by a subsequent push on the same branch (not a tenth data point
either way). Not achievable right now: the apt-hang class this investigation also surfaced (see PR #935's
`gui-smoke.yml` comments) is external and recurring — **it reproduced live again during PR #935's review,
with `Acquire::ForceIPv4=true` already applied**, same shape (silent stop right after the last InRelease
fetch, named 8:00 timeout), on the SAME `ubuntu-latest` pool this ticket's shard-2 runs also use. Root
cause is therefore NOT fixed and no run demonstrates otherwise. What IS proven, twice, including live
during this review: a hang now fails as a bounded, named, ~8-minute failure with diagnostics, instead of
a silent ~30-minute kill with none — that is this ticket's AC1/AC2 delivered; AC "ten consecutive greens"
stays open until the external stall itself is resolved (tracked more broadly in follow-up CPE-1787).
