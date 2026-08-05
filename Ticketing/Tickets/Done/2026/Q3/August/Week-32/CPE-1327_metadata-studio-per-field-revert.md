---
id: CPE-1327
title: "Metadata Studio: per-field revert + 'Reset all edits'"
type: feature
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-05
epic: CPE-725
---

## Summary
Metadata Studio stages pending edits before a Save. There's no way to undo a single field's edit or drop all
pending edits without closing the dialog. Add a **per-field revert** affordance (a changed field shows a small
revert control that restores its original value) and a **"Reset all edits"** action that drops every pending
edit and returns the form to the loaded-from-disk values.

## Build
- In `MetadataStudioDialog.svelte`, when a field's staged value differs from its original loaded value, show a
  small revert affordance on that field; clicking it restores the original value and clears that field's
  pending edit.
- Add a **"Reset all edits"** control that clears all pending edits across the form (and across the selection
  if `applyToAll`) back to the loaded values. It does NOT touch the file — it only discards unsaved edits.
- Reuse the existing original-vs-edited state the dialog already tracks; do not introduce a parallel source of
  truth. If a small pure helper clarifies "is this field dirty / original value for field", factor it and unit
  test it.
- Purely a client-side edit-state operation — no backend, no new deps.

## Acceptance criteria
- A changed field shows a revert control; clicking it restores that field's original value and removes its
  pending edit (other fields' edits untouched).
- "Reset all edits" returns every field (and all files under `applyToAll`) to loaded values with no pending
  edits, without writing to disk.
- Reverting/resetting never triggers a save or a checkpoint (those only happen on Save).
- `npm run check` clean; vitest covers per-field revert (only that field resets) and reset-all (all pending
  edits cleared, no write). i18n keys in ALL 12 locales. No new deps.

## Notes
- FRONTEND-ONLY — merge on the Frontend CI job.
- **Serializes behind CPE-1326** (same component `MetadataStudioDialog.svelte`). Build after 1326 merges.
