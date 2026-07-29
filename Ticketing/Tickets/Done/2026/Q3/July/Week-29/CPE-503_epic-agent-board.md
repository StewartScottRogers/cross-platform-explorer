---
id: CPE-503
title: "EPIC: Agent Board — a Kanban that dispatches agents, synced with the ticket system"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-16
closed: 2026-07-16
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

## Decisions (activation 2026-07-16, with the user)
- **Backing store:** the **real `Tickets/` folders** — the board reads/writes the actual `CPE-*.md`
  files; a card move moves the file. Single source of truth, consistent with the CLI `/ticketing-*`.
- **Dispatch:** an **explicit "Dispatch" action** on a card (not auto-launch on drag) — a drag only
  changes status; Dispatch launches a scoped session. Predictable, no surprise agent spawns.
- **Agent choice:** an **agent/provider/model chooser prefilled with the last-used** choice.
- **First wave:** **read + drag board** (columns + move status) first; agent dispatch is wave 2.

## Child tickets (created at activation)
Wave 1 — the board (sprint **[[SPR-03]]**):
- [[CPE-520]] — Backend: read `Tickets/` as cards + move a card between columns *(ready)*
- [[CPE-521]] — Kanban UI: columns + drag a card to change status *(needs-prereq CPE-520)*

Wave 2 — dispatch (sprint SPR-04, later):
- [[CPE-522]] — Dispatch a card to a scoped agent session (chooser + CPE-313 injection)
- [[CPE-523]] — Agents update cards + a review column

Suggested order: CPE-520 → CPE-521 (wave 1), then CPE-522 → CPE-523 (wave 2).

## Resolution (closed 2026-07-16)
The Agent Board shipped across 4 children / 2 sprints, backed by the **real `Tickets/` files** (single
source of truth with the CLI):
- **Wave 1 (SPR-03):** [[CPE-520]] backend (`board_cards` / `board_move`, pure card+status logic),
  [[CPE-521]] Kanban UI (columns + drag-to-move, opened from a Sidebar entry).
- **Wave 2 (SPR-04):** [[CPE-522]] Dispatch (card → scoped AI Console session via the CPE-313 hand-off;
  launcher supplies the last-used chooser), [[CPE-523]] virtual Review lane + `board_review` /
  `board_note` (agent card-updates + review before Done).

Move a card → its status changes; Dispatch a card → an agent opens scoped to that ticket; mark it for
Review → it waits for sign-off — all reading/writing the actual ticket markdown, so the board and
`/ticketing-*` never diverge. ~19 tests across the backend + model; clippy clean both feature modes;
534 frontend tests green.

**Follow-on (recorded):** a tighter session-chip↔card correlation and a fuller agent-driven auto-update
(beyond `board_note` + file-refresh) are natural next steps, and the board pairs well with the Swarm
coordinator ([[CPE-502]]) as a dispatch surface. Neither blocks this epic.

## Notes
From [[CPE-500]]. Uniquely feasible here because the ticket system already exists ([[CPE-487]] workflow).

## Work Log (close)
2026-07-16 — **Closed.** All 4 children Done across SPR-03 (read+drag board) + SPR-04 (dispatch+review).
The board is a real, usable Kanban over `Tickets/` that dispatches agents and moves cards through review,
consistent with the CLI. Moved Epics/ → Done/.

## Work Log
2026-07-16 — Filed as a dormant `Proposed` brief (from spike CPE-500).
2026-07-16 — **Activated** into sprint SPR-03. Resolved the open questions with the user (see Decisions:
real-folder backing · explicit Dispatch · last-used-default chooser · read+drag first). Decomposed into
4 children (CPE-520…523); wave-1 (520/521) assigned to SPR-03, wave-2 (522/523) to SPR-04. Status →
In Progress.
