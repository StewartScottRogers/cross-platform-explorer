---
id: CPE-745
title: Agent Watch — inline diff peek on touched rows + timeline entries
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-19
epic: CPE-727
estimate: 3-4h
---

## Summary
Child of CPE-727. Hovering a touched row (FileList) or a timeline entry (AgentTimeline) reveals the exact
hunks the agent wrote — a compact inline diff peek — from the CPE-744 diff store, using the existing diff
renderer and the CPE-405 read/write styling vocabulary.

## Scope
- A hover/focus affordance on write-annotated rows + timeline entries that have a diff available.
- A compact inline diff popover rendering added/removed lines via `diff.ts` (`annotateInline`/`inlineDiff`),
  themed (light/dark parity), scrolling within its own container for large hunks.
- Only writes get a peek (reads have no content diff — leave the CPE-405 read badge as-is).
- Keyboard-accessible; never blocks the row's existing click/select/DnD behaviour.

## Acceptance
- [ ] Hovering/focusing a write-touched row or timeline entry shows its hunks; no peek for reads/unchanged.
- [ ] Rendering reuses `diff.ts` + theme variables; large diffs scroll inside the popover.
- [ ] Existing row interactions (select, context menu, DnD) and the timeline are unregressed.
- [ ] Component/headless tests where feasible for the peek gating.

## Notes
Prereq: CPE-744 (diff store + selector). Frontend-only.
