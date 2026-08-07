---
id: CPE-1376
title: "Dual-pane: pane B ignores search/filter, folder-sizes, cut-highlight, and color-tag filter"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (pane-B parity audit, gaps 1/4/5/6)

Diffing the pane-A vs pane-B `<ExplorerPane>` instantiation in `src/App.svelte` (pane A ~L4945–5014;
pane B ~L5022–5048) shows pane B is missing several display props/events that pane A has:

1. **search + file-type filter** not passed → `ExplorerPane` defaults them to `""`/`"all"`, so pane B
   always shows the unfiltered listing regardless of what's typed / selected.
4. **show folder sizes** — `showFolderSizes`/`folderSizes` props + `on:needSizes` not wired → sizes never
   populate in pane B.
5. **cut-highlight** — `cutPaths` not passed → Ctrl+X dim styling never shows in pane B.
6. **color-tag filter** — `bind:selectedTag` + `on:filterTag` not wired → tag filter unusable in pane B.

## Fix direction

Pass the missing props to pane B's `<ExplorerPane>`; where the state is per-pane (selectedTag), give pane B
its own `selectedTagB` and wire `on:filterTag` to it; wire `on:needSizes` to the same `fillFolderSizes` for
pane-B entries. Keep pane A behaviour identical. **Conflict surface: the pane-B block in App.svelte — this
serializes with CPE-1371/1377/1378 (same block).** Add vitest coverage (precedent: `App.filterReset.test.ts`)
asserting pane-B `visible` respects search/filter and pane-B rows get cut/size/tag state.
