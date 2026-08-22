---
id: CPE-1863
title: the retention byte-cap loop destroys checkpoints while reclaiming nothing
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

`snapshot_prune::apply`'s byte-cap loop tracks progress as `total = total.saturating_sub(freed)`. When a
prune frees **nothing**, `freed == 0`, so `total` never falls, the cap is never seen as met, and the loop
runs all the way to its `kept.len() <= 1` floor.

Measured by the independent Security Auditor during CPE-1861, on a store with **no tamper at all** — six
identical captures, so every blob is shared and pruning any one manifest frees nothing:

```
apply(cap = total - 1) -> kept = 1, pruned = 5, bytes_freed = 0
```

Five checkpoints destroyed. Zero bytes reclaimed. The cap it was trying to meet was never going to be met
by deleting them, because they were not what was using the space.

## Reachability

**Not reachable from the app today.** `snapshot_run_due` passes `None` for the byte cap, and no caller in
`src/` passes `maxTotalBytes`. The behaviour is byte-identical on `main` — CPE-1861 neither introduced it
nor changed it, though that ticket widens the set of stores where `freed == 0` is the normal outcome.

So this is a trap waiting on whoever wires the byte cap to a setting, not a live defect. It is filed
because the loop reads as correct and the failure is silent: it reports success, having deleted the user's
history without helping.

## Acceptance criteria

- [ ] A pass that frees nothing must stop rather than continue to the floor. Decide what "no progress"
      means and record it — the honest reading is that if pruning the oldest candidate frees zero bytes,
      pruning the next one probably will not either, because shared blobs are shared by everything.
- [ ] `bytes_freed: 0` with a non-empty `pruned` list should be surfaceable as the anomaly it is. Note
      that **nothing in `src/` consumes `RetentionApplyResult` at all** (only `bindings.gen.ts` names it),
      so today there is no consumer to surface it to — say whether that is in scope here or belongs with
      CPE-1862.
- [ ] Test the no-tamper fixture above: identical captures, a cap below the total, and assert the loop
      does **not** run to the floor. Red-proof it — it must fail today.
- [ ] Check the interaction with CPE-1861's accepted leak: a store containing an ignored manifest file has
      blobs pinned that no prune can free, so `freed == 0` is the *expected* outcome there. The fix must
      not turn that into a stall.
- [ ] Say what the loop should do when the cap genuinely cannot be met — stop and report, or prune to the
      floor and report honestly that the cap was not met. Reporting success either way is what this ticket
      is about.

## Notes

Found by the independent Security Auditor during CPE-1861's audit, which recommended merge and filed this
as one of four non-blocking follow-ups. Its own framing: pre-existing shape, widened set.

Read CPE-1861's Work Log first — its `manifests_naming` witness is what decides whether a blob is freeable,
and this loop's behaviour depends entirely on that answer.

Related: CPE-1861 (the witness), CPE-1844 (`index.json` steering the same retention decision), CPE-1862
(the unreconciled index in the same subsystem).
