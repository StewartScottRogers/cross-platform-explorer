---
id: CPE-787
title: Pure workspace / layout-session model
type: feature
status: Open
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed:
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
- [ ] Parse tolerates malformed input and drops invalid entries; serialize round-trips.
- [ ] CRUD add/rename/remove/update behave correctly and immutably.
- [ ] `pruneMissing` keeps only existing-path tabs; unit tests cover all; check + suite green.

## Notes
Mirror the `smartFolders.ts` list-store shape. Foundation for CPE-788/789. Headless.
