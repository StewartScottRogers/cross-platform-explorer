---
id: CPE-796
title: Pure backup model + incremental planner
type: feature
status: In Progress
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed:
epic: CPE-736
estimate: 1-2h
---

## Summary
Foundation for backup jobs (epic CPE-736). A pure module (`src/lib/backup.ts`): a `BackupJob` model and an
incremental `planBackup(source, dest, mirror)` that computes what a run would transfer — reusing the CPE-777
folder-tree diff — so the copy engine (CPE-797) and dashboard (CPE-798, dry-run) are thin.

## Scope
- `planBackup(source: CompareNode[], dest: CompareNode[], mirror=false): BackupPlan` where
  `BackupPlan = { copy[], update[], delete[], unchanged }` (relative file paths): source-only → copy,
  changed → update, identical → unchanged, dest-only → delete (only when `mirror`). Recurses; dirs implicit.
- `BackupJob { id, name, source, dest, mirror }` + CRUD + tolerant parse/serialize.
- Pure + total.

## Acceptance Criteria
- [x] `planBackup` classifies copy/update/delete/unchanged correctly incl. nested dirs; delete only in mirror mode.
- [x] `BackupJob` CRUD immutable; parse tolerant; serialize round-trips.
- [x] Pure + dependency-light; unit tests cover planning + CRUD + parse; check + suite green.

## Notes
Reuses `treeDiff.ts` (CPE-777). Foundation for CPE-797/798. Headless.

## Resolution
Added `src/lib/backup.ts` (pure): `planBackup(source, dest, mirror)` reuses CPE-777 `diffTrees` (dest→source)
to produce `{copy, update, delete, unchanged}` relative-path lists (delete only in mirror mode; recurses
into subdirs; whole new subtrees copied); `BackupJob` CRUD + tolerant parse/serialize. 6 tests. check 0/0.
Headless; reuses treeDiff. Foundation for CPE-797/798.

