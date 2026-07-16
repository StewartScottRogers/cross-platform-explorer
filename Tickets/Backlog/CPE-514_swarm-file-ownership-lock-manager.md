---
id: CPE-514
title: "Swarm — file-ownership lock manager (path-glob exclusive claims + shared-dep sequencing)"
type: Feature
status: Open
priority: Medium
component: Sidecar
tags: [ready]
estimate: 3-4h
created: 2026-07-16
epic: CPE-502
sprint: SPR-01
---

## Summary
The **safety substrate** of Swarm ([[CPE-502]], wave 1): concurrent agents must **never collide on
files**. A task exclusively **claims path globs** (e.g. `src/auth/**`) it owns for its duration; an
overlapping claim is refused or queued; shared dependencies are auto-sequenced. Pure + unit-testable in
isolation before any multi-agent UI exists.

## Acceptance Criteria
- [ ] A task can `claim(globs)` and exclusively own matching paths; a second overlapping claim is
      **refused or queued**, never granted concurrently.
- [ ] `release()` frees a claim; a queued waiter then acquires it (FIFO / documented order).
- [ ] Glob overlap detection is correct (prefix, `**`, siblings) — non-overlapping claims run in parallel.
- [ ] Shared dependencies (a path two tasks both need) are **sequenced**, not clobbered.
- [ ] Pure core, comprehensively unit-tested (overlap matrix, queue, release, deadlock-avoidance note).

## Notes
Wave 1 of [[CPE-502]]; the foundation CPE-517 (coordinator) builds on. Path-glob granularity per the
activation decision.
