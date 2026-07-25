---
id: CPE-487
title: "Epics as a separate queue with just-in-time decomposition"
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
Epics should be a **separate queue**, not Backlog leaves. A dormant epic must sit as a one-page brief
(goal + rough scope + open questions) and do **no** research, planning, or sub-ticketing until it is
**activated** (pulled from the queue). Activation is the *only* moment an epic is decomposed: research
+ resolve decisions + create child tickets, then work the children normally. This dogfoods the change
in the ticketing skills themselves (user request, 2026-07-16).

## Acceptance Criteria
- [x] A dedicated `Tickets/Epics/` queue exists; epics live there (never in `Backlog/`), with a
      `Proposed` status while dormant and `In Progress` once activated.
- [x] A `ticketing-epic` skill provides `list` / `activate CPE-NNN` / `close CPE-NNN`; **activate** is
      the sole decomposition step (research → resolve decisions → create `epic:`-linked children in
      Backlog → set the epic In Progress). No decomposition happens while an epic is `Proposed`.
- [x] `ticketing-new` routes epic-sized items into `Epics/` as a brief (no child tickets up front);
      `ticketing-work` refuses to build an epic and redirects to `ticketing-epic activate`.
- [x] `ticketing-list` shows an **Epics** section (separate queue) with child-progress for active ones.
- [x] `Tickets/wiki.md` and `CLAUDE.md` document the Epics queue, the `Proposed` status, and the
      just-in-time lifecycle.

## Resolution
Epics are now a first-class **separate queue** with just-in-time decomposition, dogfooded in the
ticketing skills themselves.

- **Queue + status:** new `Tickets/Epics/` folder (with `README.md` explaining it); new `Proposed`
  status (epics-only, dormant brief) added to the frontmatter vocabulary in `Tickets/wiki.md`, plus a
  full **"Epics"** lifecycle section (Proposed → activate → In Progress → Done) and an optional
  `epic: CPE-NNN` child-link frontmatter field.
- **New skill** `.claude/commands/ticketing-epic.md` — `list` / `activate` / `close`. **activate** is
  the *only* decomposition step: research first happens here, `needs-decision` questions are put to the
  user, then `epic:`-linked children are filed into `Backlog/` and the epic is set `In Progress`.
  `close` verifies all children are Done (or carves out a deliberately-deferred child with the user's
  ok) before moving the epic to `Done/`. Invariants: no decomposition while `Proposed`; epics are never
  in `Doing/` and never built by `/ticketing-work`.
- **Wiring:** `ticketing-new` gains an "Epic fork" (Step 3c) that files epic-sized items into `Epics/`
  as a brief and stops (no up-front children); its auto-intercept preamble and next-ID scan were
  updated. `ticketing-work` gains Step 0, an epic guard that redirects to `ticketing-epic activate`.
  `ticketing-list` always shows an **Epics** section with child-progress. `CLAUDE.md`'s command table,
  folder table, and "showing open tickets" guidance all include the Epics queue.

Docs-only change (skills + wiki + CLAUDE.md + one folder); no app code, so no build/test step applies.

Tradeoff: the two existing epics (CPE-261, CPE-429) were already closed under the *old* model with
their children pre-enumerated, so they were left in `Done/` rather than retrofitted — the new workflow
applies to epics filed from here on.


## Work Log
2026-07-16 — Picked up (self-filed from a user request). Estimate 1-2h. Plan: add the `Tickets/Epics/`
queue + `Proposed` status, a new `ticketing-epic` skill (list/activate/close), and wire the existing
skills (new/work/list) + wiki + CLAUDE.md to treat epics as a just-in-time-decomposed separate track.
2026-07-16 — Implemented all five ACs (see Resolution). Files: new `Tickets/Epics/README.md`, new
`.claude/commands/ticketing-epic.md`; edited `ticketing-new.md`, `ticketing-work.md`,
`ticketing-list.md`, `Tickets/wiki.md`, `CLAUDE.md`. Verified the new skill auto-registers (appears in
the available-skills list); no feature-set manifest to update. Closing.

## Notes
Motivation: up-front epic breakdown rots as scope drifts; deferring all research/planning/sub-ticketing
to activation keeps the backlog honest (only actionable leaves) and the epic a stable headline.
