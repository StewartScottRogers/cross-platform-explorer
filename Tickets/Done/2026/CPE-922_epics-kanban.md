---
id: CPE-922
title: Agent Board — Epics view as a kanban (columns + archive, like tickets)
type: feature
component: Frontend
priority: medium
tags: ready
created: 2026-07-23
status: Done
---

## Summary
The Agent Board's **Epics** view is a list+detail (pick an epic → see its child tickets). Make it a
**kanban** matching the tickets board: epics laid out as cards across **Backlog / Doing / Done** columns
(mapped from their Proposed / In Progress / Done status), with an **archived** toggle on Done for closed
epics in dated `Done/**` subfolders — just like the tickets board. Each epic card shows its id, title,
status, and progress (done/total child tickets). Clicking an epic card drills into the Board view filtered
to that epic's tickets.

## Acceptance Criteria
- [x] Epics view renders kanban columns Backlog/Doing/Done with epic cards placed by status.
- [x] Done column has a "+N archived" toggle showing closed epics from the archive, like tickets.
- [x] Epic card shows id, title, status, done/total progress; clicking drills to its tickets.
- [x] Pure model helpers unit-tested; `npm run check` + board tests pass.

## Work Log
- 2026-07-23 — Filed + started.

- 2026-07-23 — Rebuilt the Epics view as a kanban (groupEpicsByColumn/epicColumn/archivedEpics/filterEpics in board.ts + tests); Backlog/Doing/Done columns, Done archive toggle, epic cards with progress bar, click-to-drill (filterCards now matches the epic field). Updated 06-agent-board.md. check + 19 board tests green.
