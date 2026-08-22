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

- [x] Measure per-spec durations rather than assuming which specs are heavy. The assignment lives in
      `scripts/write-shard-manifest.ts`; establish what shard 2 actually holds before changing anything.
- [x] Rebalance so the slowest shard is close to the mean, or record why the current split is right and
      the imbalance is inherent (e.g. one spec that cannot be split and dominates).
- [x] If rebalancing is by measured duration, say what happens when a spec's runtime changes — a
      hand-tuned split rots silently. Prefer something self-correcting, or state the maintenance cost.
- [x] Confirm shard assignment stays deterministic. Shards must not reshuffle between the build job and
      the shard jobs, or a spec could run twice or not at all.
- [x] Report before/after wall-clock for all four shards from a real CI run, not a local estimate.

## Notes

Recorded across two reviews (CPE-1832 and CPE-1843) rather than found by one. Both reviewers reached the
same reading independently, and the second explicitly checked that the PR's own diff could not explain it.

This is the tail of an observation, not a defect report — file it, but do not let it displace real work.

Related: CPE-1171 (the sharded GUI smoke design), CPE-1753 (build once for every shard), CPE-1843 (where
the second sighting was measured).

## Work Log

**2026-08-22 — PR #997, head `f0bd67c0`. The strong reading was right, and half of it was fixable.**

**What shard 2 actually held.** Not "the heaviest specs" plural — one spec. Per-spec durations came from
CI history rather than argument: each run's `gui-smoke-results-ubuntu-shard-<n>` artifact contains the
`@wdio/json-reporter` chunks, and each `wdio-*.json`'s top-level `start`/`end` is one spec file's
in-session wall time. Mean of three consecutive green runs (`32585350872`, `32589428833`, `32592641384`):

| spec | in-session, mean of 3 |
|---|---:|
| `samples.smoke.ts` | **479.3 s** (479.5 / 479.7 / 478.8 — spread 0.9 s over three runs) |
| `preview-pane.smoke.ts` | 18.2 s |
| `network.smoke.ts` | 16.2 s |
| `saved-search.smoke.ts` | 12.0 s |
| the other 37 | 1.3–4.0 s |
| **all 41 spec files** | **611.5 s** |

**One spec file is 78% of the whole suite**, and shard 2 held it. The other nine specs on shard 2 were
unremarkable. Three further sightings were confirmed in the process (in-session spans 12.67 / 12.67 /
12.70 min against 4.7–5.7 for the other three), so the observation now stands at six.

A second measured number decided how coarse the fix should be: each spec also pays a **fixed ~29.5 s of
session setup/teardown** (`span − Σ durations` per shard: 29.9 / 29.0 / 30.6 / 29.0 s). For 40 of the 41
specs that fixed cost dwarfs the spec's own work, so *counting* specs is already the correct cost model
for them — a full 41-entry measured table would have bought nothing and rotted 41 ways.

**The rebalance.** `assignShardSpecs` now costs each spec at session overhead + measured-or-default
runtime and longest-processing-time-first bin-packs onto the least-loaded shard. With one spec at 78% of
the total, that gives `samples.smoke.ts` a shard of its own and deals the other 40 evenly. Four shards
remains correct for a *new* reason: with the heavy file isolated the other three sit below the floor it
sets, so a fifth shard would shorten only jobs that are not the long pole.

**Before / after, both from real CI runs** — job wall-clock, before = run `32592641384` (main), after =
this PR's own run `32604214778`:

| job | before | after |
|---|---:|---:|
| shard 1 | 7m06s | **9m31s** ← now `samples.smoke.ts` alone |
| shard 2 | **14m02s** | 8m18s |
| shard 3 | 7m16s | 7m41s |
| shard 4 | 6m23s | 8m50s |
| **longest single shard** | **14m02s** | **9m31s** (−4m31s, −32%) |

In-session spans, like-for-like: **5.62 / 12.67 / 5.45 / 4.78 → 7.98 / 6.73 / 6.16 / 7.51 min**. The long
pole is now 7.98 min against a 6.16 min minimum, i.e. within one heavy-spec floor of the mean, and it
beat the 8.48 min prediction. The remaining gap is `samples.smoke.ts` itself and is genuinely inherent:
no partition and no shard count can put one file in two places.

**Determinism — verified in production, not just asserted.** The after-run's verdict job reported
`manifests received from shard(s): 1, 2, 3, 4` and `41/41 spec file(s) reported, 119 case(s) — 92 passed,
25 failed, 25 known-failing listed` — **identical to the pre-change runs**. Nothing ran twice, nothing
ran nowhere. Pinned by a test that runs the real `scripts/write-shard-manifest.ts` in **four separate
child processes** and joins their manifests the way the verdict job does, deliberately *not* by computing
the partition twice in one process (which passes even when the answer depends on the clock).

**How it rots, stated rather than wished away.** There is **no self-correcting static proxy** — that was
checked, not assumed. `it()` count, line count and byte count were all measured against the durations
above and all three fail: `samples.smoke.ts` is 3 top-level `it()` blocks and 186 lines (mid-pack on every
static measure, because it generates one case per file in `samples/` at load time), while
`preview-pane.smoke.ts` has the *most* `it()` blocks (8) and is 26× faster. So the table is measured and
hand-maintained, with the cost bounded:

- a stale entry, or a new heavy spec nobody lists → **balance degrades only**; the partition stays a
  bijection, so correctness is never at stake.
- an entry naming a renamed or deleted spec → **reds** (the one rot a static check can see).
- a slowest shard more than one spec-slot past the floor → **reds**, against the live `specs/` directory.
- re-measurement is a five-minute `gh run download` recipe, written into `lib/shard.ts` so the table is
  updated by measurement rather than by argument.

**Gates.** `npx vitest run` 325 files / 4340 passed / 0 failed; `npm run check` 0 errors 0 warnings;
gui-smoke `npm run test:unit` 38 suites / 130 passed / 0 failed (was 126); gui-smoke `npm run typecheck`
clean; `gui-smoke.yml` parses under PyYAML with the matrix still `[1, 2, 3, 4]`.

**Red-proof, one line each, both observed red and both reverted.**
1. `let target = 0;` → `let target = Date.now() % shardTotal;` — the cross-process determinism test reds
   ("spec file(s) claimed by more than one shard"); 4 failures.
2. `return byWeight !== 0 ? byWeight : compareSpecNames(a, b);` → `return compareSpecNames(a, b);` — the
   balance test reds on the real spec set; 2 failures.

**Not verified.** The predicted-vs-actual comparison rests on a single after-run; the before side has
n=6. If the next few runs put a different shard on top, the weight table is the thing to re-measure. The
Windows gui-smoke leg is unsharded and untouched.
