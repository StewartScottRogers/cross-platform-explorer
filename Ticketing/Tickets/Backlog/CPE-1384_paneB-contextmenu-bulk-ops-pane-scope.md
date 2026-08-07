---
id: CPE-1384
title: "Dual-pane: batch-rename / batch-media / copy-to / move-to / archive / vault from a pane-B menu act on pane A's scope"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (CPE-1377 follow-up)

CPE-1377 made the pane-B context menu's core actions (open/rename/copy-path/delete) pane-aware, but the
heavier bulk operations — batch-rename, batch-media, copy-to, move-to, archive, secure-vault — invoked from a
pane-B row still operate on **pane A's** selection/target scope. They're hidden where a gating prop exists and
"degrade safely" (don't corrupt) elsewhere, but from the user's seat they either do nothing or act on the
wrong pane.

## Fix direction

Route these bulk-op entry points through the same `paneStateFor(ctx.inPaneB)` mechanism CPE-1377 established,
so a pane-B menu operates on pane B's selection + folder. Audit each op (`batchRename`, `batchMedia`,
`copyTo`/`moveTo`, `archiveSelection`, vault) for a live-pane read and switch it to the ctx-snapshot pane.
Keep the CPE-1370 delete-snapshot safety model for any destructive op. Touches `src/App.svelte` — **shares the
pane-B / ctx dispatch, serialize with other App.svelte pane-B work.** Add per-op tests proving pane-B scope.
