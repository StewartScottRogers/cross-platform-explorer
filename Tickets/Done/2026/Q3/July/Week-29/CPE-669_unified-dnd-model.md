---
id: CPE-669
title: Unify internal drag-and-drop into one shared model
type: feature
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
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
- [x] `src/lib/dnd.ts` owns draggable/valid-target/effect logic; pure parts unit-tested.
- [x] FileList (all view modes) + Sidebar + SidebarNode delegate to it; ad-hoc handlers removed.
- [x] Copy/move follows the OS convention with a modifier override; drop highlight + count badge themed.
- [x] No regression to existing internal drag copy/move; `npm run check` + suite green.

## Work Log
2026-07-18 (nightshift) — Picked up (prereq CPE-668 landed). Best-guess decisions, no questions. Estimate 3-4h.

## Resolution
New `src/lib/dnd.ts` owns the shared model: `setDragData`, `isValidDrop` (self/descendant rule),
`resolveEffect` (OS convention — Ctrl=copy, Shift=move, else same-volume=move/cross=copy, unknown→copy),
and `hoverEffect` (modifier-driven cursor). FileList (all view modes incl. gallery — same component) and
Sidebar/SidebarNode now delegate to it; their ad-hoc validTarget/effect logic is gone. The drop dispatch
now carries `{ctrlKey, shiftKey}` instead of a pre-decided `copy`; `App.dropInto` resolves the effect via
`same_volume` (CPE-668) + `resolveEffect`. Multi-select drags show a themed count badge (`setDragBadge` +
`dnd.itemCount` ×12 locales). 7 dnd unit tests; check clean; suite green (666); vite bundle clean.

Assumption logged (no-questions nightshift): the hover cursor is modifier-driven and the same-volume
auto-decision is resolved at drop (one IPC round-trip), not pre-resolved per hovered target — exact
per-target hover prediction is a deferred polish item. Live GUI drag verification recommended on /run.
Files: src/lib/dnd.ts(+test), src/lib/components/FileList.svelte, Sidebar.svelte, src/App.svelte,
src/lib/i18n.ts.
