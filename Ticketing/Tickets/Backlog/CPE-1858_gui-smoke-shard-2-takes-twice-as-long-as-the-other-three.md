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

---

**2026-08-22 — independent review: APPROVED, 11 findings, 0 blocking. Five corrections to the record
below; none change the code.**

The reviewer downloaded all 123 artifact files from the three cited runs and recomputed every per-spec
figure from scratch. The table above matches **to the decimal**; `samples.smoke.ts` is **78.4%** of the
suite; the 40 non-`samples` specs mean **exactly 3.30 s**, which is `DEFAULT_SPEC_RUNTIME_MS`. Nothing
was rounded in the ticket's favour. It also verified the bijection under 8 adversarial shapes and 200
random input permutations, and found LPT's makespan **equals the brute-force optimum** on the real set.

**Correction 1 — "beat the 8.48 prediction" was a measurement artifact, not a result.** Shard 1 now
holds a *single* spec, so its `max(end) − min(start)` span **is** that spec's own duration and contains
no session overhead at all — while the 8.48 prediction included one overhead slot. The two are not the
same quantity, so 7.98 < 8.48 measured nothing. Like-for-like the prediction was **accurate, not
beaten**. The honest comparison is job wall clock: **9m31s against a predicted ~9.8 min**, which does
hold. Corrected in `README.md`. A model that looks better than it is gets trusted further than it should.

**Correction 2 — the cross-process determinism test is *probabilistic* against the clock mutation.**
Re-running the `Date.now() % shardTotal` red-proof seven times, the reviewer saw 2, 3 and 4 failures on
different runs: in six of seven *that* test redded, and in the seventh it went **green** while the
balance test redded instead. The cause is real — the four children spawn about a second apart and 1000
is 0 mod 4, so their `Date.now() % 4` can coincide across all four and yield four mutually-consistent
manifests. Something redded 7/7, so **the guard set holds**; the individual test catches it *usually*.
The original write-up cited it as a deterministic red, which was wrong, and the test's own comment now
says so. This is a statement about the **test**, not about the risk: a real clock dependency in
production faces four runners starting *minutes* apart, where that coincidence does not save you.

**Correction 3 — "there is no self-correcting static proxy" was overstated.** True, and properly
demonstrated, for proxies over a spec's own **source**. But `samples.smoke.ts` emits one case per file
under `samples/` — 48 files, less READMEs and the separately-run `malformed.pdf` = 46 cases, 479.3 / 46 =
**10.4 s each** — so a count of that *tree* would track its dominant term self-correctingly. Adopting it
would mean filesystem I/O inside a deliberately-pure module plus a per-spec special case in the one
function all four jobs must agree on, so the design call stands; the **claim** is now scoped to
source-based proxies instead of stated absolutely, with the rejected option written down as the door to
reopen.

**Correction 4 — the balance test checks the model against itself.** `assertBalanced` computes both the
loads **and** the bound from `specWeightMs`. It therefore reds on a regression in the **partitioning
algorithm** and is blind, by construction, to the **table** drifting from reality: halve
`samples.smoke.ts`'s real runtime or triple `preview-pane.smoke.ts`'s and it stays green while the shards
quietly un-balance. That is precisely the "balance degrades only" mode documented above — but the earlier
wording ("a slowest shard more than one spec-slot past the floor reds, against the live `specs/`
directory") could be read as catching real-world drift, and it does not. Now said explicitly in all three
places: read it as *"the packer still packs"*, never *"the shards are still balanced in CI"*. Only a
re-measurement closes that loop.

**Correction 5 — shard 4's margin is thin and nothing watches it.** It carries all three runner-up specs
(`preview-pane` + `network` + `saved-search`, 18.2 + 16.2 + 12.0 s) and lands only ~28 s of in-session
time (~40 s of job time) behind the long pole. If those three grow, **shard 4 becomes the long pole** and
neither the table nor any test notices — the leg just gets slower. Recorded in the cost-model block and
the README: re-measure when any of *them* grows, not only when `samples.smoke.ts` does.

**What the review strengthened.**

- The ~29.5 s session overhead could not be reproduced exactly, but it is **bracketed** by two defensible
  definitions of the same quantity: 26.1–27.5 s dividing the artifact spans, 29.5–31.9 s dividing the
  workflow *step* duration (which also carries per-step setup the artifacts never see). The design
  conclusion holds under **either** by 1.5–20×, so the constant sitting at the boundary does not matter.
  Noted in both the cost-model block and the README.
- Determinism was attacked well beyond the test: Turkish locale `LC_ALL=tr_TR.UTF-8` (the `localeCompare`
  trap this module's sort exists to avoid), `LC_ALL=C`, `TZ=Pacific/Kiritimati`, and running from the
  repo root instead of `gui-smoke/` — **all produced identical partitions**.
- No `actions/checkout` in `gui-smoke.yml` carries a `ref:`, so the build job, all four shards and the
  verdict resolve the **same immutable SHA**. A spec file cannot be added between jobs.
- The four manifests were pulled straight out of production artifacts: **1 + 14 + 13 + 13 = 41, all
  unique, `samples.smoke.ts` alone on shard 1** — stronger evidence than any test in the branch.
- One pre-existing inconsistency found and cleared, not fixed: `wdio.conf.ts:46` resolves `specs/` from
  `__dirname` while `write-shard-manifest.ts` and `run-ratchet.ts` use `process.cwd()`. They agree in CI,
  and a wrong cwd makes `listSpecFiles` **throw** rather than silently disagree — it fails closed. Worth
  a note, not a change.
- A sharper number for **CPE-1866** (filed on this ticket's session-overhead finding): after the change,
  shard 2 spends **6.73 min of in-session span on 0.47 min of actual test — about 93% overhead**.
