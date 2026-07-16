---
id: CPE-519
title: "Swarm — cost/token budgets, failure-retry, and coordinator authority"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-16
epic: CPE-502
sprint: SPR-02
---

## Summary
Guardrails for running N concurrent agents ([[CPE-502]]): per-mission / per-agent **cost & token
budgets**, a **failure/retry** policy, and the **coordinator's authority** to pause, stop, or reassign a
misbehaving or over-budget agent.

## Acceptance Criteria
- [ ] A configurable budget (tokens/cost) per mission and per agent; exceeding it pauses + surfaces, never
      silently overspends.
- [ ] A defined retry policy on task/agent failure (bounded retries, then escalate to the coordinator).
- [ ] The coordinator can pause / stop / reassign any agent; actions are visible + auditable.
- [ ] Tests for budget enforcement + the retry/escalation state machine.

## Notes
Wave 2 of [[CPE-502]]. **needs-prereq:** [[CPE-517]]. Not in SPR-01.
