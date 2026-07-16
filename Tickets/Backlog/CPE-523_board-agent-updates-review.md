---
id: CPE-523
title: "Agent Board — agents update cards + a review column"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [needs-prereq]
epic: CPE-503
sprint: SPR-04
estimate: 2-3h
created: 2026-07-16
---

## Summary
Wave 2 of [[CPE-503]]: as a dispatched agent works, its **card reflects progress** (state / appended
findings), and finished work lands in a **Review** column before Done — closing the "agents move cards
through review" loop from the brief.

## Acceptance Criteria
- [ ] A dispatched agent's progress updates its card (status and/or an appended findings note).
- [ ] A **Review** step/column exists between Doing and Done; a card lands there for sign-off before Done.
- [ ] Updates stay consistent with the `Tickets/` files + the CLI flow (no divergent state).
- [ ] Tests for the card-update + review-transition logic.

## Notes
**needs-prereq:** [[CPE-522]] (dispatch). Ties to the Swarm quality-gate idea ([[CPE-518]]).
