---
id: CPE-522
title: "Agent Board — Dispatch a card to a scoped agent session"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [needs-prereq]
epic: CPE-503
sprint: SPR-04
estimate: 3-4h
created: 2026-07-16
---

## Summary
Wave 2 of [[CPE-503]]: an explicit **Dispatch** action on a card (activation decision — not auto-launch
on drag) that launches a **scoped AI Console session** on that ticket, reusing task injection
([[CPE-313]]). On dispatch, an **agent chooser** defaults to the last-used agent/provider/model.

## Acceptance Criteria
- [ ] A card has a **Dispatch** action; it opens an agent chooser prefilled with the last-used choice.
- [ ] Confirming launches an AI Console session scoped to the ticket (its content injected as the task
      via CPE-313), and moves the card to Doing.
- [ ] The launched session is correlated to the card (session chip / ticket id) so it's traceable.
- [ ] Never auto-launches from a plain drag (explicit action only).
- [ ] Tests for the dispatch payload (ticket → task injection) + last-used default.

## Notes
**needs-prereq:** [[CPE-520]]/[[CPE-521]] (board) + [[CPE-313]] (task injection). Explicit-dispatch +
chooser per the activation decision.
