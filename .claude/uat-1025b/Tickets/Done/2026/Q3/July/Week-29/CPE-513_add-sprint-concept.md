---
id: CPE-513
title: "Add a time-boxed Sprint concept to the ticket system + always show Epics & Sprints when listing"
type: Task
status: Done
priority: Medium
component: Docs
tags: [ready]
estimate: 1-2h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
User preference (stated 2026-07-16): "When I ask for tickets, make sure to show me **epics and
sprints**. Remember this in a skill." Sprints don't exist yet. User chose a **real, time-boxed Sprint**
concept (not a derived view). Add the Sprint concept to the ticket system, a `/ticketing-sprint`
management skill, and make ticket listings always include an **Epics** section AND a **Sprints** section.

## Acceptance Criteria
- [x] A Sprint concept exists: `Tickets/Sprints/` folder, `SPR-NN` ids, a `sprint: SPR-NN` frontmatter
      field on member tickets, and a status lifecycle (Planned → Active → Closed).
- [x] A `/ticketing-sprint` skill manages them: `list` / `new` / `activate` / `close` / `assign` / `remove`.
- [x] `/ticketing-list` always shows a **Sprints** section (Active + Planned, with progress) in addition
      to the existing Epics section.
- [x] `Tickets/wiki.md` documents the Sprint concept (folder, ids, field, lifecycle).
- [x] `CLAUDE.md` command table + "Showing open tickets" rules updated to require Epics & Sprints.
- [x] The preference is saved to memory so it persists across sessions.

## Resolution
Added a first-class, time-boxed **Sprint** concept and made ticket listings always surface **Epics AND
Sprints** (user preference, 2026-07-16).

- **Concept:** `Tickets/Sprints/` (new queue, `.gitkeep`), ids `SPR-NN` (separate sequence from
  `CPE-NNN`), membership via a `sprint: SPR-NN` frontmatter field on member tickets (authoritative +
  countable), statuses **Planned → Active → Closed** (Closed moves to `Done/`). Orthogonal to epics —
  a ticket can carry both `epic:` and `sprint:`.
- **`/ticketing-sprint` skill** (`.claude/commands/ticketing-sprint.md`, new): `list` / `new` /
  `activate` / `close` / `assign CPE-NNN [SPR-NN]` / `remove`, mirroring the `ticketing-epic` shape
  (one Active at a time; a sprint never works tickets itself, it's a lens).
- **`/ticketing-list`**: added a mandatory **Sprints** table (Active first, with `X of Y Done` progress)
  after the Epics table.
- **`Tickets/wiki.md`**: Sprints in the folder structure, the `SPR-NN` id rule, the `sprint:` field in
  the frontmatter reference, and a full "Sprints (a separate queue, time-boxed)" section.
- **`CLAUDE.md`**: `/ticketing-sprint` in the command table, `Tickets/Sprints/` in the folder table, and
  the "Showing open tickets" rule now requires Blocked + Deferred + **Epics** + **Sprints** (item 5).
- **Memory:** saved `ticketing-show-epics-and-sprints` (feedback) + MEMORY.md index line so it persists.

Docs/skills only — no app code, so no build/test gates apply. No sprint was pre-created (none warranted
yet); the first is one `/ticketing-sprint new` away.

## Work Log
2026-07-16 — Picked up. User wants Sprints as a first-class, time-boxed tracked concept + always shown when listing tickets.
2026-07-16 — Built the concept (Sprints/ folder, SPR-NN, sprint: field, lifecycle), the /ticketing-sprint skill, the /ticketing-list Sprints section, wiki + CLAUDE.md updates, and the persisted memory. All ACs met.
