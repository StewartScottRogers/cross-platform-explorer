---
id: CPE-514
title: "Swarm — file-ownership lock manager (path-glob exclusive claims + shared-dep sequencing)"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [ready]
estimate: 3-4h
created: 2026-07-16
epic: CPE-502
sprint: SPR-01
closed: 2026-07-16
---

## Summary
The **safety substrate** of Swarm ([[CPE-502]], wave 1): concurrent agents must **never collide on
files**. A task exclusively **claims path globs** (e.g. `src/auth/**`) it owns for its duration; an
overlapping claim is refused or queued; shared dependencies are auto-sequenced. Pure + unit-testable in
isolation before any multi-agent UI exists.

## Acceptance Criteria
- [x] A task can `claim(globs)` and exclusively own matching paths; a second overlapping claim is
      **refused or queued**, never granted concurrently.
- [x] `release()` frees a claim; a queued waiter then acquires it (FIFO / documented order).
- [x] Glob overlap detection is correct (prefix, `**`, siblings) — non-overlapping claims run in parallel.
- [x] Shared dependencies (a path two tasks both need) are **sequenced**, not clobbered.
- [x] Pure core, comprehensively unit-tested (overlap matrix, queue, release, deadlock-avoidance note).

## Resolution
Added `sidecar/ai-console/src/swarm_locks.rs` (new, pure, 7 tests) — the Swarm no-collision substrate.

- **Glob intersection** `path_globs_overlap(a, b)` decides whether two path globs can match a common
  concrete path, via recursive pattern-intersection non-emptiness: `**` matches zero-or-more whole
  segments, `*` matches within a segment, literals compared char-wise. Correctly handles prefix scopes
  (`src/**` ⊃ `src/auth/x.rs`), leading `**` (`**/*.rs` vs `src/lib.rs`), and rejects siblings
  (`src/a/**` vs `src/b/**`) and cross-segment `*` (`src/*.rs` vs `src/sub/lib.rs`).
- **`LockManager`**: `claim(task, globs)` → `Granted` when nothing active overlaps, else **`Queued`**
  (FIFO). `release(task)` frees the claim and grants any now-clear waiters front-to-back (an earlier
  waiter wins a contested path), returning the newly-granted task ids. A **shared dependency** (a path
  two tasks both want) is therefore **sequenced**, never clobbered; disjoint claims run in parallel.
  Query helpers: `is_held` / `is_queued` / `active_tasks` / `queued_tasks`.
- Fully pure (no I/O, no real files); exported from the crate. Deadlock note: claims are all-or-nothing
  per task and granted only when fully clear, so no partial-hold cycle forms.

Verified: `cargo clippy --all-targets -D warnings` clean; 7 unit tests pass (overlap matrix, parallel,
queue, release-grants-waiter, FIFO contention, selective unblent). First ticket of sprint SPR-01.

## Work Log
2026-07-16 — Picked up (SPR-01 wave 1; foundation of CPE-502). Estimate: 3-4h.
2026-07-16 — Built the pure glob-intersection + LockManager (path-glob exclusive claims, FIFO queue, shared-dep sequencing) with 7 tests. clippy clean. All ACs met.

## Notes
Wave 1 of [[CPE-502]]; the foundation CPE-517 (coordinator) builds on. Path-glob granularity per the
activation decision.
