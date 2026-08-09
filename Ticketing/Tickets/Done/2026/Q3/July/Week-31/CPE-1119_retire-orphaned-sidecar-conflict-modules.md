---
id: CPE-1119
title: "Retire or document the orphaned sidecar conflict modules"
type: chore
component: Backend
priority: low
status: Deferred
tags: cleanup
created: 2026-07-26
epic: CPE-730
---

## Summary
Five sidecar modules — `sidecar/ai-console/src/{conflict,conflict_window,conflict_rename,conflict_owner,
conflict_region}.rs` — are `pub mod` (compiled + unit-tested) but **called nowhere outside their own
`#[cfg(test)]`**. They were TDD-first logic slices (CPE-914/1067/1068/1069/1070) whose integration was never
filed; the shipped conflict radar instead uses **frontend folds** (`agentConflicts.ts`, and CPE-1116/1118
mirror this logic in TS). So the Rust modules are dead code in the wrong process.

## Options (decide on pickup)
- **(a) Delete** them as superseded-by-frontend-folds (lean-core; removes dead code + its tests).
- **(b) Document** them as a deliberate future "move the fold to the backend" option (keep as reference).

Not required for CPE-730 DoD (satisfied by the frontend folds). Flagged so they aren't mistaken for live code.

## Work Log
2026-07-26 (sprint) — Filed from the CPE-730 close plan (Plan agent found the orphans). Deferred, low-pri —
our choice, pickable anytime; not blocking the CPE-730 close.

2026-07-27 (sprint) — **Done.** Option (a) DELETE: removed 5 orphaned sidecar conflict modules (946 lines) after grep-proving each was referenced only by the others + own tests; sidecar build + clippy -D + 378 tests green. PR #442 merged (Foreman-reviewed — token-conserving furlough wind-down, independent gauntlet skipped by user request).
