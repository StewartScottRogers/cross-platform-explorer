---
id: CPE-787
title: Pure workspace / layout-session model
type: feature
status: Done
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed: 2026-07-20
epic: CPE-708
estimate: 1-2h
---

## Summary
Foundation for workspaces (epic CPE-708). A pure module (`src/lib/workspaces.ts`) modelling a named set of
tabs (path + view/sort/filter), with tolerant parse/serialize, CRUD, and a restore-time prune of
moved/missing paths — so the switcher (CPE-788) and auto-restore (CPE-789) are thin.

## Scope
- `WorkspaceTab { path; view?; sortKey?; sortDir?; filter? }`, `Workspace { id; name; tabs[] }`.
- `parseWorkspaces(json)` (tolerant: bad JSON / wrong shape → `[]`, invalid entries dropped) +
  `serializeWorkspaces(list)`.
- CRUD: `addWorkspace(list, name, tabs)`, `renameWorkspace(list, id, name)`, `removeWorkspace(list, id)`,
  `updateWorkspace(list, id, tabs)`.
- `pruneMissing(ws, exists: (path)=>boolean)` → a copy with only still-existing tabs (graceful restore).

## Acceptance Criteria
- [x] Parse tolerates malformed input and drops invalid entries; serialize round-trips.
- [x] CRUD add/rename/remove/update behave correctly and immutably.
- [x] `pruneMissing` keeps only existing-path tabs; unit tests cover all; check + suite green.

## Notes
Mirror the `smartFolders.ts` list-store shape. Foundation for CPE-788/789. Headless.

## Resolution
Added `src/lib/workspaces.ts` (pure): `Workspace`/`WorkspaceTab` types, tolerant `parseWorkspaces`
(bad JSON/shape → [], invalid entries + tabs dropped) + `serializeWorkspaces`, immutable CRUD
(add/rename/remove/update), and `pruneMissing(ws, exists)` for graceful restore of moved/absent paths.
Mirrors the smartFolders list-store shape (`ws_` ids). 4 tests. check 0/0. Headless; no existing code
touched. Foundation for CPE-788/789.

