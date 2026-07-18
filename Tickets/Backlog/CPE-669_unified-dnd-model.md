---
id: CPE-669
title: Unify internal drag-and-drop into one shared model
type: feature
component: Frontend
priority: high
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-661
estimate: 3-4h
---

## Summary
Consolidate the per-component drag/drop logic (FileList, Sidebar, SidebarNode) into one shared helper
`src/lib/dnd.ts`: what is draggable, what is a valid drop target, and the copy-vs-move decision using the
OS convention (same-volume=move via CPE-668, cross-volume=copy, modifier overrides). Every file view
(details/list/icons/gallery) and the sidebar reuse it, with a consistent themed drop-target highlight and
a drag badge showing the item count. Preserve full multi-select payloads. Prereq: CPE-668.

## Acceptance Criteria
- [ ] `src/lib/dnd.ts` owns draggable/valid-target/effect logic; pure parts unit-tested.
- [ ] FileList (all view modes) + Sidebar + SidebarNode delegate to it; ad-hoc handlers removed.
- [ ] Copy/move follows the OS convention with a modifier override; drop highlight + count badge themed.
- [ ] No regression to existing internal drag copy/move; `npm run check` + suite green.

## Work Log
