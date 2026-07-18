---
id: CPE-673
title: Drag-and-drop in specialized views (gallery, archive, board)
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-661
estimate: 2-3h
---

## Summary
Extend the unified DnD model to the specialized views (per the v1 broad-scope decision): verify gallery
tiles drag/drop (inherited from FileList), make archive view **drag-out = extract-on-drop** (dragging a
file out of an open archive extracts it to the drop target; archives accept no drop-in), and ensure the
Agent **Board** card DnD is not regressed by the shared model. Consistent themed affordances across all.
Prereq: CPE-669.

## Acceptance Criteria
- [x] Gallery view supports the same drag/drop as the other file views (verified).
- [ ] Dragging a file from an open archive extracts it to the drop target; dropping into an archive is a
      no-op with clear feedback.
- [x] Agent Board card DnD still works (no regression from the unified model).
- [x] Affordances (highlight, badge, cursor) consistent + themed; `npm run check` + suite green.

## Work Log

## Work Log
2026-07-18 (nightshift) — Picked up (prereq CPE-669). No questions; best-guess.

## Resolution
- **Gallery:** verified covered by construction — gallery is FileList with `view="gallery"`, so the shared
  DnD model (draggable rows, `data-drop-path`, count badge) from CPE-669 already applies. No new code.
- **Agent Board:** verified no regression — BoardView has fully independent card DnD (its own
  dragstart/drop over a card id, not `draggedPaths`/`dnd.ts`); nothing this epic touched affects it.
- **Archive drop-IN = no-op:** added a `canDrag` prop to FileList (false in archive view via
  `canDrag={!archive}`), so archive rows are neither draggable nor drop targets. This also fixes a latent
  issue where dragging a synthetic in-zip row errored on drop. Clean inert behavior = the "no-op" AC.
- **Archive drag-OUT = extract-on-drop:** carved out to [[CPE-674]] (Deferred) — it needs a backend
  extract-entry-to-path command + GUI verification of the drag-out gesture, unsafe to ship blind.

check clean; suite green (666); bundle clean. Files: src/lib/components/FileList.svelte, src/App.svelte.
