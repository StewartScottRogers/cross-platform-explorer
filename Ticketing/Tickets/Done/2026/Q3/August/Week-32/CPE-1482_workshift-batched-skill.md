---
id: CPE-1482
title: "Add /workshift-batched — a bounded workshift wrapper with a max-batch-count stop condition"
type: Feature
status: Done
priority: Medium
component: Tooling/Skills
tags: [ready]
created: 2026-08-08
closed: 2026-08-08
---
## What
A new `.claude/commands/workshift-batched.md` skill: the autonomous `/workshift` loop, but **bounded by a
max batch count** so it can be started before leaving (work / sleep) and winds down cleanly on its own
instead of running indefinitely. Requested by the user 2026-08-08 ("a real workshift-batch skill with a
proper stop condition, limited by a max batch count, sized to let me go to work or sleep").

## Design (safe, bounded — the opposite of the rejected infinite-loop pseudo-skills)
- **Batch = one completed shippable unit**: a ticket merged through the gauntlet, a ticket *parked* by the
  circuit-breaker (a completed decision), or — when the queue is dry — one PM/Researcher increment that
  produces new ready work.
- **`max_batches` ceiling** (default 40; sizing table for sleep/workday/long-day in the skill). Persisted
  counter at `.claude/workshift-metrics/BATCH-COUNTER` that **survives session-resets** (the resuming
  session reads it and continues the same count), so the bound covers the whole run across checkpoints.
- **Stop = whichever first**: count reached (wrap + teardown + delete counter) · safe work genuinely
  exhausted (STOP EARLY, don't pad) · budget reset line (checkpoint + hand-off, count continues) · user
  returns (presence check).
- **Honesty clause**: `max_batches` is a **ceiling, not a quota** — never manufacture filler to hit it; stop
  early and report if real purpose-fitting work runs out. This is precisely why the earlier unbounded
  `while True` / bare-except / remote-sync pseudo-skills were declined (twice): no stop condition, unbounded
  disk/CPU/network, un-killable. This skill is their safe, real replacement.

## Why not the 16 pseudo-skill variants as written
Most (GPU scheduler, VR dashboard, remote-checkpoint-sync, parallel-thread spinners) don't map to a ticket
workflow and/or were resource-exhaustion footguns (infinite loops, unbounded log growth, un-killable
bare-except, unbounded outbound POSTs). One well-designed bounded skill delivers the actual intent — "keep
working while I'm away, then stop" — safely.

## Verify
Additive markdown skill (no code compile). Cross-checked: name doesn't collide with existing
`.claude/commands/` (only `workshift.md` present) or the concurrent `workshifts_*` family (CPE-1476);
references `workshift.md` + PURPOSE.md correctly; defers to `workshift.md` on any conflict except the bound.

## Work Log
- 2026-08-08 (workshift, Foreman-authored): wrote `.claude/commands/workshift-batched.md`; filed + closed
  this ticket; initialized a BATCH-COUNTER for the current run per the user's request to keep working while
  away. Coordinated with concurrent CPE-1476 (distinct name).
