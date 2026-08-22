---
id: CPE-1858
title: gui-smoke shard 2 takes twice as long as the other three, on every run
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

The GUI smoke suite runs across four parallel shards. Shard 2 consistently takes roughly **twice** as
long as the others, so the job's wall-clock is set by one shard while three sit idle.

Three consecutive sightings, on runs where nothing plausibly causal changed:

| run | shard 1 | shard 2 | shard 3 | shard 4 |
|-----|---------|---------|---------|---------|
| CPE-1832's fix | ~7 min | **~14 min** | ~6 min | ~7 min |
| CPE-1843 round 1 (`32555323418`) | 7m06s | **14m21s** | 7m10s | 6m28s |
| CPE-1843 round 2 (`32561465109`) | ~7 min | **~14 min** | ~6 min | ~6 min |

All green both times. CPE-1843's only functional change was a `cargo install` version pin, which cannot
affect spec runtime — which is what makes this look like a stable property of the shard assignment rather
than noise.

The first sighting was recorded during CPE-1832's review and explicitly not chased, on the reasoning that
a single observation could be runner variance. Three sightings, none with a causal change in the diff,
make "shard 2 owns the heaviest specs" the settled reading rather than a hypothesis.

## Why Low

Nothing is broken and nothing is flaky — all four shards pass. The cost is wall-clock: the job takes about
14 minutes when it could take about 8, on every PR, and CI queue depth has been the throughput bottleneck
for this batch.

## Acceptance criteria

- [ ] Measure per-spec durations rather than assuming which specs are heavy. The assignment lives in
      `scripts/write-shard-manifest.ts`; establish what shard 2 actually holds before changing anything.
- [ ] Rebalance so the slowest shard is close to the mean, or record why the current split is right and
      the imbalance is inherent (e.g. one spec that cannot be split and dominates).
- [ ] If rebalancing is by measured duration, say what happens when a spec's runtime changes — a
      hand-tuned split rots silently. Prefer something self-correcting, or state the maintenance cost.
- [ ] Confirm shard assignment stays deterministic. Shards must not reshuffle between the build job and
      the shard jobs, or a spec could run twice or not at all.
- [ ] Report before/after wall-clock for all four shards from a real CI run, not a local estimate.

## Notes

Recorded across two reviews (CPE-1832 and CPE-1843) rather than found by one. Both reviewers reached the
same reading independently, and the second explicitly checked that the PR's own diff could not explain it.

This is the tail of an observation, not a defect report — file it, but do not let it displace real work.

Related: CPE-1171 (the sharded GUI smoke design), CPE-1753 (build once for every shard), CPE-1843 (where
the second sighting was measured).
