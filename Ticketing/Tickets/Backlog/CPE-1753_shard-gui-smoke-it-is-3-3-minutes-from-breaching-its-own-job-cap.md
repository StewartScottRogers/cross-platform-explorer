---
id: CPE-1753
title: Shard the GUI-smoke suite — it is 3.3 minutes from breaching its own job cap, and no timeout value survives the growth rate
type: task
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-15
closed:
---

## Problem

Escalated by the PR #912 (CPE-1728) reviewer, 2026-08-15, from measurements taken while sizing a step-level
timeout. The conclusion was that **no timeout value survives**, and the reason is arithmetic rather than
judgement.

From PR #912's own green CI run (31871682587, head `856dbebc`):

```
success  29.93m  Run GUI smoke suite (xvfb-run)
success   0.02m  Classify suite log
success   0.00m  Ratchet — no new GUI regressions
success   0.03m  Upload gui-smoke screenshots
         41.70m  JOB TOTAL          (cap: 45)
```

A **fully healthy green run sits at 41.70 of 45 minutes.**

### The suite scales with spec count — it is not near-constant

Bucketing suite duration by the spec count at each run's own head SHA:

| specs | runs | min | median | max |
|---:|---:|---:|---:|---:|
| 40 | 6 | 27.50 | **28.15** | 29.25 |
| 41 | 4 | 29.57 | **29.95** | 30.42 |

The ranges are **disjoint** (40-spec max 29.25 < 41-spec min 29.57). One added spec cost **+1.80 min** at
the median; flat-averaged it is 0.73 min/spec. True marginal cost is in **[0.73, 1.80] min per spec**.

A three-day sample at a constant 41 specs had looked near-deterministic (stdev 0.102 min) — which is why
this needed the older runs to see. A stable-looking sample at a fixed input size cannot distinguish
"genuinely stable" from "clamped".

### Spec-count history on `main`

**2026-08-01: 22 → 2026-08-08: 39 → 2026-08-15: 41.** Nineteen spec files in fourteen days.

At 0.73-1.80 min per spec, **2 to 4 more spec files breach the 45-minute job cap.** When that happens the
job dies mid-suite on every run and reds every PR — and unlike CPE-1728's cancellations, `if: always()`
cannot rescue it, because there will be no verdict left to report. This is CPE-1728 again in a form its own
fix cannot reach.

### Why a step-level timeout is not an option

Tried in PR #912 and removed on review. To fire before the job cap the step cap must be
`< 45 − 11.9 = 33.1` (11.9 min is the measured max setup-before-suite), and to leave the reporting tail
room, `<= ~32.5`. The usable window is **[30.4, 32.5] — about two minutes wide, narrower than one spec
file's growth.** A 32-minute cap had a 1.58-minute margin: **0.9 to 2.2 spec files.**

## What to do

Shard `gui-smoke-linux` across parallel jobs so the per-job wall clock drops well under the cap and stops
tracking spec count one-for-one. The prize is bigger than the cap: at ~10 minutes per job, a second push
within the window would rarely land mid-suite, which is the **actual** cliff behind CPE-1728 (`concurrency`
+ `cancel-in-progress: true` against a 41-minute job — any second push within ~41 minutes kills the
verdict, and no timeout value changes that).

Design considerations for whoever picks this up:

- The ratchet's completeness check (`expectedSpecCount` globbed live from disk, `reportedSpecCount <
  expectedSpecCount` ⇒ `incomplete`) is **per-run today**. Sharding splits `.results/` across jobs, so the
  verdict has to be assembled from all shards — and a **missing shard must be a red**, not a smaller
  expected count. That is the single highest-risk part of this change: it is exactly the shape that turns a
  check into a silently-passing one.
- `known-failing.json` exemptions are global; the stale-exemption check ("listed but no longer failing")
  can only run once every shard has reported.
- The artifact upload and `suite-output.log` need a per-shard name or they will collide.
- Keep the `if: always()` verdict behaviour CPE-1728 landed — it must survive per shard.

## Acceptance criteria

- [ ] `gui-smoke-linux` completes in comfortably under half the job cap at the current spec count, and its
      per-job duration no longer grows one-for-one with total spec count. Record before/after.
- [ ] A **missing or cancelled shard** produces a red verdict, not a reduced expected count. Prove it by
      dropping a shard's results and observing the exit code — this is the CPE-1728 lesson applied to the
      new shape.
- [ ] A genuinely new failing case in **any** shard still reds, with the same actionable message.
- [ ] Stale-exemption detection still works across the full set of shards.
- [ ] The suite log and screenshots are retrievable per shard (no artifact-name collisions).
- [ ] The measured numbers in this ticket are re-taken after the change and written into the workflow
      comment, replacing the "3.3 minutes from the cap" warning.

## Notes

Related: CPE-1728 (PR #912 — the `if: always()` verdict, the classifier, the artifact upload, and the
removed step cap), CPE-1266 (the original timeout/concurrency work, closed), CPE-1594/CPE-1048 (this leg
being the blocking gate, and the Windows leg's status).
