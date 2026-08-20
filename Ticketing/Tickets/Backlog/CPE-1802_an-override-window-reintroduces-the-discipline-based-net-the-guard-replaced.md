---
id: CPE-1802
title: an ffmpeg pin override window reintroduces the discipline-based net the guard just replaced
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1796 added a mechanical guard so the ffmpeg pin must be a **month-end anchor** — upstream retains
those indefinitely, whereas rolling dailies are pruned after about 14 days and eventually 404 the
release build. It also lowered the freshness check's cadence from twice-weekly to weekly, reasoning
that the guard now blocks accidental dailies so the check no longer needs to catch them early.

That reasoning holds **except during a deliberate override window.** When someone sets
`FFMPEG_BUILD_TAG_OVERRIDE_REASON` and pins a daily on purpose, the pin genuinely is a daily again with
the original ~14-day runway — and the mitigation on offer is "remember to dispatch the freshness check
manually".

So for exactly the case this ticket family exists to make mechanical, the safety net becomes a
discipline someone has to remember, at the one moment they are already doing something unusual and
under time pressure.

## What to do

- Make the override **arm its own safety net**. The cleanest shape suggested during review: trigger
  `ffmpeg-pin-freshness.yml` automatically when `FFMPEG_BUILD_TAG_OVERRIDE_REASON` transitions to
  non-empty — a `repository_dispatch`, or a step in the guard job itself that fires the check when it
  takes the override path.
- Consider also raising the cadence *only while an override is active*, rather than globally. A
  conditional schedule is awkward in GitHub Actions, so weigh that against simply running the check on
  every push while the override is set — during an override window that is a small number of runs and
  the exposure is real.
- Whatever the shape, the point is that **the person who takes the override should not also have to
  remember the consequence.** If it stays manual, say so explicitly next to the override variable so
  the obligation is visible where the decision is made, rather than in a workflow comment they will
  never open.

## Notes

Filed by the Foreman from PR #956's review, 2026-08-20, which flagged it as a judgment call rather than
a bug and explicitly out of scope for that PR. The guard itself was verified not to fire on any
legitimate pin — leap years, month lengths, malformed and missing values all handled — so this is about
the one path that deliberately steps around it.

Related: **CPE-1796** (the guard and the cadence change), **CPE-1789** (the repin that prompted it),
**CPE-1763** (the freshness check), **CPE-1795** (its recommendation bug).
