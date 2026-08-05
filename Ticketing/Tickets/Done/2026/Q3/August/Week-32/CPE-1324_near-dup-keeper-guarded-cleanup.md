---
id: CPE-1324
title: "Near-duplicate review: keeper-guarded 'Move to Bin' cleanup in NearDuplicatesDialog"
type: feature
component: frontend
priority: high
status: Done
tags: ready
created: 2026-08-05
epic: CPE-997
---

## Summary
`SimilarImagesDialog.svelte` has a full **keeper-guarded** cleanup workflow (per-item selection, a delete that
is disabled unless ≥1 item is kept per group, a best-effort checkpoint before deleting, group pruning on
success). Its sibling `NearDuplicatesDialog.svelte` (near-duplicate **documents + folders**) is **read-only** —
you can find near-dups but not act on them. Bring it to parity so users can actually clean up near-duplicate
documents/folders safely.

## Build
- Add per-item selection (checkboxes) + a **"Move to Bin"** action to `NearDuplicatesDialog.svelte`, mirroring
  the proven pattern in `SimilarImagesDialog.svelte`. **Reuse the existing helpers** in `src/lib/duplicates.ts`
  (`keepsOnePerGroup` / `pruneGroups`) and the existing commands (`commands.deleteToTrash`,
  `commands.checkpointCreate`) — do NOT write new backend or new dup logic.
- **Keeper guard (hard safety):** the "Move to Bin" button is **disabled** unless at least one item per group
  is kept — it must be *impossible* to arm a delete-everything-in-a-group. This is the core safety property;
  copy it faithfully from SimilarImagesDialog.
- **Checkpoint first:** take a best-effort `checkpointCreate` before deleting (as SimilarImagesDialog does); a
  checkpoint failure must NOT block the delete (best-effort, logged).
- **Prune on success:** remove the deleted items from their group in the UI; if a group drops to ≤1 item, drop
  the group.
- Optional convenience (fold in): a **"Select extras"** button that selects all-but-the-first per group — but
  it must never be able to arm a delete-all (the keeper guard still applies). Show a live selected-count.
- Light-theme palette + existing dialog styling. Pills/badges (if any) follow the reflow convention.

## Acceptance criteria
- In NearDuplicatesDialog, the user can select near-duplicate documents/folders and Move-to-Bin them; the
  action is disabled whenever any group would be left with zero kept items.
- A checkpoint is attempted before deletion; a checkpoint failure does not block the delete.
- Deleted items are pruned from their group; empty/singleton groups disappear.
- `npm run check` clean; a vitest suite covers the keeper-guard (delete disabled when a group has no keeper),
  the checkpoint-before-delete call order, and prune-on-success. No new deps.

## Notes
- FRONTEND-ONLY (reuses existing commands + `duplicates.ts`) — merge on the Frontend CI job.
- Collision surface: `src/lib/components/NearDuplicatesDialog.svelte` (isolated) + append-only additions to
  `src/lib/i18n.ts` (must add keys to ALL 12 locales or `i18n.test.ts` fails).
- Docs: if this adds a user-facing capability to an existing documented section, update that `src/docs/*.md`
  page (no new `sectionDocs.ts` entry unless it's a new section).
- Reference implementation to mirror: `src/lib/components/SimilarImagesDialog.svelte`.
