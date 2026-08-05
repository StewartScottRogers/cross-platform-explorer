---
id: CPE-1326
title: "Metadata Studio: batch 'Strip editable metadata' + 'Copy from first'"
type: feature
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-05
epic: CPE-725
---

## Summary
Media Metadata Studio can edit metadata across a selection but has no batch conveniences. Two high-value
batch ops, both backed by existing primitives (`buildMetaEdits` in `src/lib/metaEdits.ts` already emits a
`clear` edit; `metadataWrite` exists): **Strip editable metadata** (clear all editable fields across the
selection) and **Copy from first** (apply the primary/first file's editable values to the rest).

## Build
- Add two actions to `MetadataStudioDialog.svelte`:
  - **Strip editable metadata** — clears all *editable* (writable) fields for the target set. Use the existing
    `clear`-edit path in `metaEdits.ts` (`buildMetaEdits`) — do NOT invent a new clear mechanism.
  - **Copy from first** — takes the primary file's editable field values and stages them as edits on every
    other file in the selection. Respects each file's writability (skip non-writable fields/files) and the
    existing `applyToAll` semantics.
- Both must respect the existing per-format **writability** gating (`metadataWritable` / `is_writable`) — never
  stage an edit for a field/format that can't be written.
- Both stage edits into the existing edit/save flow (so the CPE-1325 checkpoint-before-save still fires) — they
  do NOT write directly, they populate the pending edits then the user Saves (or wire through the existing save
  path consistently — match how the dialog currently applies edits).
- Extend `src/lib/metaEdits.ts` only if a shared helper genuinely clarifies "copy primary's values to others";
  add a unit test in `metaEdits.test.ts` for any new pure logic.

## Acceptance criteria
- "Strip editable metadata" clears every editable field across the selection (verified: staged edits are
  `clear` for all writable fields; non-writable fields untouched).
- "Copy from first" stages the primary's editable values onto the other selected files, respecting writability.
- Neither op writes a field for a format that doesn't support it.
- `npm run check` clean; vitest covers the new pure logic (strip → all-clear edits; copy → primary values
  propagated, non-writable skipped); i18n keys added to ALL 12 locales. No new deps.

## Notes
- FRONTEND-ONLY — merge on the Frontend CI job.
- **Serializes on `MetadataStudioDialog.svelte`** — this is the same component as CPE-1325 (already merged) and
  the follow-on CPE-1327 (per-field revert). Build this before 1327.
- Consider adding a `gui-smoke` spec that opens the Metadata Studio and asserts the new batch buttons render,
  so the Visual Critic has a screenshot (no spec exists today) — optional but valued.
