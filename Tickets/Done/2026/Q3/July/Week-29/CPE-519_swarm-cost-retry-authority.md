---
id: CPE-519
title: "Swarm — cost/token budgets, failure-retry, and coordinator authority"
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
Guardrails for running N concurrent agents ([[CPE-502]]): per-mission / per-agent **cost & token
budgets**, a **failure/retry** policy, and the **coordinator's authority** to pause, stop, or reassign a
misbehaving or over-budget agent.

## Acceptance Criteria
- [x] A configurable budget (tokens/cost) per mission and per agent; exceeding it pauses + surfaces, never
      silently overspends.
- [x] A defined retry policy on task/agent failure (bounded retries, then escalate to the coordinator).
- [x] The coordinator can pause / stop / reassign any agent; actions are visible + auditable.
- [x] Tests for budget enforcement + the retry/escalation state machine.

## Resolution
Extended the coordinator (`swarm_coordinator.rs`) with the guardrails for running N concurrent agents.

- **Budgets** (`Budget { max_tokens, max_cost_millis }`, `0` = unlimited; cost in milli-dollars to stay
  integer): `set_budget(mission, per_agent)` + `on_usage(agent, tokens, cost)` accumulate per-agent and
  mission-wide spend. Exceeding the **per-agent** cap **pauses that agent**; exceeding the **mission**
  cap **pauses all dispatch** (`advance` returns nothing). Never silently overspends — it stops and
  records the reason; `resume_agent` / `resume_mission` lift the pause. `spend()` exposes the totals.
- **Retry:** `set_max_retries(n)`; `on_failed` now increments a per-task attempt count and **reopens**
  (retry) while under the limit, then **escalates** to a terminal `Failed` — every outcome audited.
  Default `0` retries preserves the prior fail-fast behaviour.
- **Coordinator authority:** `pause_agent` / `resume_agent`, `stop_agent` (pause + fail its running
  task, which then follows the retry policy), `reassign(task, agent)` (re-route a not-yet-running task).
  `advance` skips paused agents. Every authority + budget action lands in an **`audit()`** trail;
  `is_agent_paused` / `is_mission_paused` / `attempts_of` expose state.

Verified: `cargo clippy --all-targets -D warnings` clean; 15 coordinator tests (4 new: retry-then-
escalate, per-agent-budget-pause, mission-budget-pause, pause/reassign/stop authority) + **233 ai-console
lib tests** pass. Final ticket of SPR-02 — **completes the Swarm epic CPE-502**.

## Work Log
2026-07-16 — Picked up (SPR-02 wave 2, final). Estimate: 2-3h.
2026-07-16 — Added Budget + on_usage (per-agent + mission caps → pause), retry-then-escalate in on_failed, and authority (pause/resume/stop/reassign) with an audit trail; advance skips paused agents / a paused mission. 4 new tests. Fixed two test expectations to match the fixed-assignment model. clippy clean; 233 crate tests pass. All ACs met.

## Notes
Wave 2 of [[CPE-502]]. **needs-prereq:** [[CPE-517]]. Not in SPR-01.
