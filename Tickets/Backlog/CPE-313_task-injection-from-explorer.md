---
id: CPE-313
title: Task/prompt injection from explorer context
type: Feature
status: Open
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-13
---

## Summary

The reason the console lives inside a file explorer: act on what you're looking at.
Right-click a folder/selection → "Work on this with <agent>", which opens a console
session in that repo seeded with a task referencing the selection. Uses the context
capability ([[CPE-267]]) so the explorer stays decoupled.

## Acceptance Criteria

- [ ] Explorer context action hands the current folder/selection to the console via
      the context capability — no direct coupling to console internals.
- [ ] Opens/starts a session scoped to that repo, pre-filled with the task/selection.
- [ ] Works from folder and multi-selection; degrades cleanly if no agent installed.
- [ ] The explorer feature compiles/ships even with the console absent (delete-test).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-267]], [[CPE-289]]. **Phase:** C2 (basic) → C5 (rich).
**Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening. This is the explorer↔console payoff.
