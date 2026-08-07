---
id: CPE-1367
title: "File-type filter leaks across tabs/folders — loadPath doesn't reset fileFilter"
type: Bug
status: Done
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
closed: 2026-08-06
---

## Problem (found by the CPE-1366 follow-up audit of per-tab state isolation)

Setting the toolbar file-type filter (e.g. "Images") in one tab/folder carried it into every OTHER tab
and into new/duplicated/reopened tabs — often showing an empty pane in a folder full of documents, with the
filter dropdown still reading "Images" though the user never set it there. Recoverable (reset to "All"),
but confusing and can hide a folder's real contents.

## Root cause

Same class as CPE-1366: `fileFilter` (App.svelte:336) is a **bare top-level `let`**, transient view state
(NOT a persisted preference like `sort`/`view`/`foldersFirst`, which are saved to settings). Its filter
siblings `search` and `selectedTag` are cleared by `loadPath`'s `if (!keepSelection)` block as
"folder-scoped", but `fileFilter` was left out — so it stuck globally and bled across tabs (tabs store only
`{id, history}`; every tab op funnels through `loadPath`, which reset archive/smartFolder/structuredSearch/
selection/search/selectedTag but not fileFilter).

Per-tab isolation audit result: `fileFilter` was the ONLY leak; selection, search, selectedTag,
smartFolder, structuredSearch, archive (CPE-1366), activeMetaColumns (restored per-folder), error/loading
are all correctly reset/restored, and sort/view/showHidden/foldersFirst are global-by-design (persisted).

## Fix

Reset `fileFilter = "all"` in `loadPath`'s `if (!keepSelection)` block alongside `search`/`selectedTag`, so
the file-type filter is folder-scoped like its siblings and can't bleed across tabs.

## Tests

NEW `src/App.filterReset.test.ts` — drives the real App: apply the "Images" filter in a folder (a `.txt`
row hides), navigate, assert the row returns (filter cleared). Verified it FAILS without the fix and PASSES
with it. `npm run check` clean.

## Work Log

- 2026-08-06 — Follow-up to CPE-1366: audited all per-tab view state for the same leak class; `fileFilter`
  was the one real leak. Fixed at the loadPath chokepoint + validated regression test. Bundles into the
  next build.
