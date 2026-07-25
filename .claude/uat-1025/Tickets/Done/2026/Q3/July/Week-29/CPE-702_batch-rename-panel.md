---
id: CPE-702
title: Batch Rename panel (GUI) + preview, undo, docs
type: feature
component: Frontend
priority: medium
status: Done
resolution: Duplicate
created: 2026-07-18
closed: 2026-07-19
epic: CPE-699
estimate: 3-4h
---

## Summary
Child of CPE-699. The user-facing shell over a batch-rename engine and backend apply command: a Batch
Rename panel opened from a multi-selection, with operation rows, a live old→new preview table with
collision highlighting, and apply/cancel + undo.

## Resolution — Duplicate (2026-07-19)
**Closed as a duplicate: this panel already exists and ships today.** `src/lib/components/BatchRenameDialog.svelte`
(built across CPE-424/426/427/481/630, all Done) already provides:
- four rename modes — find/replace, add-text (prefix/suffix), sequential number, change-case
  (`src/lib/batchRename.ts` `planFindReplace` / `planAffix` / `planNumber` / `planCase`),
- a **live old→new preview** that dims no-ops and highlights conflicts,
- **conflict detection** that blocks Apply (`hasConflict` / `canApply`),
- **apply as one undoable step** via the `move_exact` backend command, wired in `App.svelte`
  (`beginBatchRename` / `applyBatchRename`, the `batch-rename` command, and the dialog render).

That satisfies CPE-702's acceptance criteria. This ticket — and its parent epic **CPE-699** plus siblings
**CPE-700** (engine) and **CPE-701** (`rename_many` backend) — were filed during the autonomous nightshift
on the **false premise** that "the app only renames one entry in-place." The Rust backend was checked, but
the existing *frontend* feature was not, so the epic duplicated shipped functionality.

Per the user's decision (2026-07-19), the duplicate nightshift code (`src/lib/rename.ts`, the `rename_many`
command + helpers, and their tests) was **reverted/removed** rather than kept, leaving a single
batch-rename implementation. The only nightshift change kept is **CPE-697** (brace-expansion glob search),
which is unrelated and legitimate.

Lesson: before filing/building a feature, grep the **whole** codebase (frontend + backend) for an existing
implementation — see the batch-rename-exists memory.

## Acceptance Criteria
- [x] Select N entries → open panel → build a recipe → preview updates live with collision highlighting →
      apply renames all N per the preview; undo reverses it. *(already provided by BatchRenameDialog.)*
- [x] `npm run check` + full suite green. *(green after the revert.)*
- [x] Docs section shipped + mapped; plain explorer unchanged when unused. *(existing feature already documented.)*

## Notes
Prereqs referenced during the nightshift (CPE-700/701) are now reverted as duplicates.
