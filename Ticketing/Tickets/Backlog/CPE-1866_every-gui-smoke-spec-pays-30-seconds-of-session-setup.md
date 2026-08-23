---
id: CPE-1866
title: every gui-smoke spec pays ~30 seconds of session setup, which is now most of the suite
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-22
closed:
---

## Problem

Each gui-smoke spec pays a fixed **~29.5 seconds** of WebDriver session setup and teardown before any of
its own work runs. Measured across three green runs during CPE-1858: 29.9 / 29.0 / 30.6 / 29.0 seconds per
shard.

For **40 of the 41 specs** that overhead dwarfs the spec itself — 37 of them do 1.3–4.0 seconds of real
work each. So the suite spends far more time starting and stopping browsers than testing.

## Why now

Before CPE-1858 this was invisible: one spec (`samples.smoke.ts`, 479 s of a 611 s suite) dominated
everything and the long pole was obviously that file. CPE-1858 gave it a shard of its own and cut the
long pole from 14m02s to about 9m30s.

Now the three light shards are **~60% session overhead**. It is the next lever, and there is no other:
the remaining imbalance is `samples.smoke.ts` itself, which no partition can split.

## Acceptance criteria

- [ ] Establish what the ~29.5 s is actually spent on before changing anything — driver start, app launch,
      first paint, teardown, artifact write. CPE-1858 measured the total; nobody has measured the parts.
- [ ] Decide whether specs can share a session, and what that costs in isolation. A shared session is
      faster and leaks state between specs; this suite exists to catch UI regressions, so a leak that makes
      a spec pass because of what ran before it would be worse than the time saved. Say which way and why.
- [ ] If sessions stay per-spec, attack the 29.5 s directly and report the parts you moved.
- [ ] Re-measure the four-shard wall-clock from a REAL CI run, before and after, the way CPE-1858 did —
      not a local estimate.
- [ ] If a shared session is taken, CPE-1858's weight table needs revisiting: its per-spec cost model is
      `session overhead + measured runtime`, and the overhead term would no longer be per spec.

## Notes

Found while measuring CPE-1858 and explicitly held out of scope there. Its worker's note: the overhead
"is now 60% of the three light shards' time and is the next lever if this leg needs shortening again."

Read CPE-1858's Work Log first — it carries the per-spec measurement recipe (`gh run download` of each
run's `gui-smoke-results-ubuntu-shard-<n>` artifact, each `wdio-*.json`'s top-level `start`/`end` as one
spec's in-session wall time), which is the same instrument this ticket needs.

Related: CPE-1858 (the rebalance), CPE-1753 (build once for every shard), CPE-1171 (the sharded design).
