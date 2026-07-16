---
id: CPE-503
title: "EPIC: Agent Board — a Kanban that dispatches agents, synced with the ticket system"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic, needs-decision]
estimate: 4h+
created: 2026-07-16
---

## Summary
Part of the **Agent Workspace** program (sibling to the AI Console [[CPE-261]]; from spike [[CPE-500]]).
A **Kanban that dispatches agents** (the BridgeBoard analogue) — two-way synced with **our own** ticket
system (`Tickets/` folders + the `/ticketing-*` workflow), which makes this uniquely natural here. A
brief only until activated.

## Goal
Move a card → an agent picks up the ticket and works it; agents file findings + move cards through
review. The board IS the CPE ticket queue, visualised and agent-driven.

## Rough scope (NOT decomposed)
- A board UI over the `Tickets/` folders (columns ≈ Backlog / Doing / Blocked / Deferred / Done).
- **Dispatch on move**: dropping a card into "Doing" launches an AI Console session scoped to that
  ticket (reuse task-injection CPE-313).
- Agents update card state / append findings as they work; a review column.

## Open questions (resolve at activation)
- Reuse the CPE ticket folders as the board's backing store, or a parallel model?
- How does an agent "pick up" a card — auto-launch a scoped session? Which agent?
- Interplay with the CLI `/ticketing-work` flow so both surfaces stay consistent (`needs-decision`).

## Notes
From [[CPE-500]]. Uniquely feasible here because the ticket system already exists ([[CPE-487]] workflow).
