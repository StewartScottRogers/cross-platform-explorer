---
id: CPE-1387
title: "Dual-pane: batch-rename/media post-apply refresh reads live paneBPath, not the snapshotted folder"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (CPE-1384 / PR #663 reviewer observation — display-only, NOT data-loss)

`applyBatchRename`/`applyBatchMedia` correctly snapshot their target `{entries, inPaneB, dir}` before the
dialog opens and MUTATE the snapshotted files safely (verified). But the post-apply **refresh** step reads
live `paneBPath` (not the snapshotted `dir`) when deciding which folder to reload. So if pane B is renavigated
to a different folder while the batch dialog is still open, the refresh reloads the new folder instead of the
one that was actually renamed — a stale/wrong-folder display. The mutation itself is fully snapshot-safe; only
the UI refresh can be off. No data loss.

## Fix direction

Use the snapshotted `target.dir`/`target.inPaneB` for the post-apply refresh (reload the folder that was
actually operated on), not live `paneBPath`. For pane B reload via `explorerPaneB?.loadListing(target.dir, false)`
guarded on the snapshot; mirror the both-panes refresh pattern if the source folder can show in both panes.
Touches `src/App.svelte` `applyBatchRename`/`applyBatchMedia`. Add a test: renavigate pane B mid-dialog, apply,
assert the refresh targets the originally-renamed folder.
