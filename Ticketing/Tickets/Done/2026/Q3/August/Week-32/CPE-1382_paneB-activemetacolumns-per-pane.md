---
id: CPE-1382
title: "Dual-pane: activeMetaColumns is keyed by pane A's path, so pane B shows the wrong custom columns"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (CPE-1378 follow-up)

CPE-1378 wired pane B's custom metadata columns, but `activeMetaColumns` is still computed/keyed off **pane A's**
`currentPath` (per-folder column config), so pane B displays pane A's active columns rather than its own
folder's. `columnWidths` is correctly shared (it's a single global width setting), but the *which columns are
active* config is per-folder and currently pane-A-only.

## Fix direction

Make `activeMetaColumns` pane-aware: compute pane B's active columns from `paneBPath`'s saved column config
(mirror how pane A derives it from `currentPath`). Pass the pane-B-specific set to pane B's `<ExplorerPane>`.
Touches `src/App.svelte` (the pane-B `<ExplorerPane>` block + the activeMetaColumns derivation) — **shares the
pane-B block, serialize with other App.svelte pane-B work.** Add a test asserting pane B shows its own folder's
columns independent of pane A.
