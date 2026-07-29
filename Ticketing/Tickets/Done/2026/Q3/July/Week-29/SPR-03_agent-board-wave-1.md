---
id: SPR-03
title: "Sprint 3 — Agent Board, wave 1 (read + drag board)"
status: Closed
start: 2026-07-16
end: 2026-07-30
created: 2026-07-16
closed: 2026-07-16
---

## Goal
Wave 1 of the **Agent Board** epic [[CPE-503]]: a Kanban view over the **real `Tickets/` folders` —
read the tickets into columns and **drag a card to change its status** (the file moves + frontmatter
updates). Read + drag only; agent dispatch is wave 2 (SPR-04).

## Tickets
Wave-1 board foundation of [[CPE-503]] (each carries `sprint: SPR-03`):
- [x] CPE-520 — Backend: read `Tickets/` as cards + move a card between columns
- [x] CPE-521 — Kanban UI: columns + drag a card to change status

Order: CPE-520 (backend) → CPE-521 (UI).

## Resolution (closed 2026-07-16)
**Goal met** — both wave-1 tickets Done. The Agent Board reads the real `Tickets/` folders into Kanban columns ([[CPE-520]] backend) and lets you **drag a card to change its status** (file moves + frontmatter updates), opened from a Sidebar entry ([[CPE-521]] UI). 11 tests (6 backend + 5 model), clippy clean both feature modes, npm check clean, 531 frontend tests. Agent dispatch continues in SPR-04 ([[CPE-522]]/[[CPE-523]]). No carry-overs.
