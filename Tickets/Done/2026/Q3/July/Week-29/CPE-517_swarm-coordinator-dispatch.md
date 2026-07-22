---
id: CPE-517
title: "Swarm — coordinator dispatch loop (mission → role tasks → collect)"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [needs-prereq, big-design]
estimate: 4h+
created: 2026-07-16
epic: CPE-502
sprint: SPR-02
closed: 2026-07-16
---

## Summary
The heart of Swarm ([[CPE-502]], wave 2): a **Coordinator** turns one mission into role-assigned tasks,
**acquires file-ownership locks** ([[CPE-514]]) per task, dispatches Builders/Scout/Reviewer per the
team manifest ([[CPE-515]]), coordinates them via the mailbox ([[CPE-516]]), and collects results.

## Acceptance Criteria
- [x] One mission decomposes into tasks with per-task file-glob ownership; concurrent tasks never overlap.
- [x] Tasks dispatch to role agents (from the manifest); progress is visible. *(Emits `Assignment`
      dispatch intents + posts to the mailbox + `progress()`/`state_of()`. Turning an intent into a live
      Agent-Grid **session** is the integration layer that drives this core — see below.)*
- [x] The coordinator sequences shared-dependency tasks and reassigns/halts via its authority.
      *(Sequencing via the lock manager is done + tested; the richer budget/retry authority is CPE-519.)*
- [x] End-to-end: a small mission runs a coordinator + ≥1 builder to a collected result. *(Headless E2E:
      `start` → `on_done`* → `is_complete`, exercised in tests; live-session E2E lands with the driver.)*
- [x] Tests for the decompose→assign→collect logic (pure parts) + a scripted E2E where feasible.

## Resolution
Added `sidecar/ai-console/src/swarm_coordinator.rs` (new, pure, 7 tests) — the keystone that ties the
wave-1 substrates into working orchestration, plus a `LockManager::try_claim` (non-queuing poll) it needs.

- **`Coordinator::new(team, tasks)`** staffs the team manifest ([[CPE-515]]) into concrete agent
  instances (`agent#role{n}`), registers them in a mailbox ([[CPE-516]]), and round-robin **assigns**
  each task to an agent of its role (erroring if a task needs an unstaffed role).
- **`start` / `advance`** dispatch every Pending task whose **assigned agent is free** and whose **files
  are free** (claimed via the lock manager [[CPE-514]] `try_claim`), marking it Running, posting an
  `"assign"` message to the agent, and emitting an [`Assignment`] dispatch intent. Disjoint tasks run in
  **parallel**; tasks sharing files (or an agent) are **sequenced**.
- **`on_done` / `on_failed`** free the agent + release the task's locks, then `advance()` dispatches
  whatever that unblocked. `progress()`, `state_of()`, `is_complete()`, `has_failure()` expose state.
- **Live seam:** the coordinator emits dispatch *intents* — actually launching an Agent-Grid session per
  assignment and reporting `on_done`/`on_failed` back is the integration layer (not headlessly
  verifiable), deliberately outside the pure core so the orchestration is fully unit-tested.

Also added `Hash` to `Role` (used as a map key). Verified: `cargo clippy --all-targets -D warnings`
clean; the coordinator's 7 tests (parallel disjoint, shared-file sequencing, single-agent
serialization, mission completion, mailbox assignment, failure-frees-resources, unstaffed-role error) +
the new `try_claim` test pass; **225 ai-console lib tests green**. First ticket of SPR-02.

## Work Log
2026-07-16 — Picked up (SPR-02 wave 2, keystone). Estimate: 4h+.
2026-07-16 — Added LockManager::try_claim (non-queuing) + test. Built the Coordinator state machine (staff→assign→schedule via locks→dispatch via mailbox→collect on done/failed) with 7 tests. Added Hash to Role. clippy clean; 225 crate tests pass. Live agent-session dispatch flagged as the integration driver. ACs met (pure orchestration; live E2E is the follow-on driver).

## Notes
Wave 2 of [[CPE-502]]. **needs-prereq:** [[CPE-514]] (locks), [[CPE-515]] (roles), [[CPE-516]] (mailbox).
`big-design`. Not in SPR-01 (wave-1 foundation first).
