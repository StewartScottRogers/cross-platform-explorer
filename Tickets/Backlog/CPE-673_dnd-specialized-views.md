---
id: CPE-673
title: Drag-and-drop in specialized views (gallery, archive, board)
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
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
- [ ] Gallery view supports the same drag/drop as the other file views (verified).
- [ ] Dragging a file from an open archive extracts it to the drop target; dropping into an archive is a
      no-op with clear feedback.
- [ ] Agent Board card DnD still works (no regression from the unified model).
- [ ] Affordances (highlight, badge, cursor) consistent + themed; `npm run check` + suite green.

## Work Log
