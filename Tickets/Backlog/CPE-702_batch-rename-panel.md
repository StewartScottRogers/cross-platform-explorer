---
id: CPE-702
title: Batch Rename panel (GUI) + preview, undo, docs
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-699
estimate: 3-4h
---

## Summary
Child of CPE-699. The user-facing shell over the CPE-700 engine and CPE-701 apply command: a Batch Rename
panel opened from a multi-selection, with operation rows (add/remove/reorder), a live old→new preview
table with collision/no-op/invalid highlighting, and apply/cancel + undo.

## Scope
- Svelte panel/dialog; operation-row editors bound to the engine's recipe; `$:` live preview via
  `applyRecipe` + `validate`. Apply calls `rename_many`; wire undo to the returned inverse map.
- Follows MENUS.md / TABS.md / tick-tacks reflow conventions; dialog gets a visible border; path/name
  inputs per conventions.
- Docs: a batch-rename section in `src/docs/*.md` + its `section → slug` entry in `sectionDocs.ts`
  (CPE-579 guard).

## Acceptance Criteria
- [ ] Select N entries → open panel → build a recipe → preview updates live with collision highlighting →
      apply renames all N per the preview; undo reverses it.
- [ ] `npm run check` + full suite green; GUI-verified (preview correctness, collision block, apply, undo).
- [ ] Docs section shipped + mapped; plain explorer unchanged when unused.

## Notes
**Attended GUI** — preview/apply/undo interaction needs live verification. Prereqs: CPE-700 (engine),
CPE-701 (apply command).

## Work Log
