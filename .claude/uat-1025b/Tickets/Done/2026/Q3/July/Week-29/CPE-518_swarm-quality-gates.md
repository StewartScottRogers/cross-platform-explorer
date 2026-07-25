---
id: CPE-518
title: "Swarm — quality gates before a task is 'done'"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-16
epic: CPE-502
sprint: SPR-02
closed: 2026-07-16
---

## Summary
A Swarm task ([[CPE-502]]) isn't **done** until it passes a **quality gate** (tests / review), so the
swarm can't mark broken work complete. Define gates + enforce them in the coordinator's close path.

## Acceptance Criteria
- [x] A gate is definable per task/team (e.g. "tests pass", "reviewer approves") and runs before close.
- [x] A failed gate keeps the task open + routes it back (to the builder / a reviewer) via the mailbox.
- [x] Gate outcomes are visible per task; a mission completes only when all gates pass.
- [x] Tests for gate evaluation + the fail-reopen path.

## Resolution
Extended the coordinator (`swarm_coordinator.rs`) with per-task **quality gates** — a task's work being
finished is no longer the same as it being **done**.

- **`Gate` per task**: `None` / `Tests` / `Review`. When a task's work completes (`on_done`), a
  `Gate::None` task goes straight to Done; a **gated** task moves to a new **`Gating`** state, **keeps
  its file locks** (so a reopen is safe and overlapping tasks keep waiting), and its gate is requested —
  a `Review` gate **posts a "review" request to the reviewers over the mailbox**, a `Tests` gate waits
  for the live driver to run the suite.
- **Gate result** arrives via `on_gate_pass` (release locks → Done) or `on_gate_fail` (**reopen**: back
  to Pending with locks retained → the coordinator re-dispatches the task to its agent to fix). `advance`
  now re-dispatches a reopened task without re-claiming its already-held lock.
- **Visibility / completion:** `state_of` surfaces `Gating`; `is_complete` requires **every** task Done,
  so a mission can't complete with a task still gating or failed.

Verified: `cargo clippy --all-targets -D warnings` clean; 11 coordinator tests (4 new: gated-awaits-gate,
failed-gate-reopens, review-asks-reviewer-via-mailbox, gating-keeps-files-locked) + the full **225
ai-console lib tests** pass. Second ticket of SPR-02.

## Work Log
2026-07-16 — Picked up (SPR-02 wave 2). Estimate: 2-3h.
2026-07-16 — Added Gate + Gating state; routed on_done through the gate (Review posts to reviewers, locks held), on_gate_pass/fail (accept / reopen-and-redispatch), advance reuses held locks on reopen. 4 new tests. clippy clean; 225 crate tests pass. All ACs met.

## Notes
Wave 2 of [[CPE-502]]. **needs-prereq:** [[CPE-517]] (coordinator close path). Not in SPR-01.
