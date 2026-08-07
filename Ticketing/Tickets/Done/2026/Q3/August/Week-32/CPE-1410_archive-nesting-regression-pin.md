---
id: CPE-1410
title: "Test: pin archive-inside-smart-folder / structured-search nesting (regression guard)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-07
---

## Problem (hardening scout, Vein A/C — regression pin, no known bug)
`enterArchive()` (App.svelte) never clears `smartFolder`/`structuredSearch` — only openSmartFolder/
openStructuredSearch/loadPath clear `archive`. So double-clicking a .zip INSIDE a smart-folder or saved-search
listing stacks `archive` ON TOP of the virtual view; `archiveOverride` correctly wins over `smartOverride` in
ExplorerPane, and `exitArchive()` correctly falls back to the still-set virtual view. This appears CORRECT by
design but NO test exercises the overlap (smart-folder and archive tests never intersect). A future edit to
openSmartFolder/enterArchive could silently break the stacking/unstacking.

## Fix direction
Add a regression-pinning test (new `src/App.archiveNesting.test.ts` or extend `App.archiveNav.test.ts`, same
pattern) that: opens a smart-folder view, enters an archive from within it (asserts archive overlays the smart
view — archiveOverride wins), then exitArchive() (asserts it falls back to the smart-folder view, not the plain
folder). Pin the CURRENT (believed-correct) behavior so a regression is caught. Test-only; if you find the
behavior is actually BROKEN, STOP and REPORT (don't fix).
