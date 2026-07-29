---
id: CPE-423
title: "Reveal a file in-app (select + scroll), not just its folder"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
created: 2026-07-15
estimate: 45m
closed: 2026-07-15
---

## Summary
Content-search (CPE-417) and duplicate (CPE-421) hits currently navigate to the file's *folder* but
leave you to find the file. Add a reusable `revealFileInApp(path)` that navigates to the parent AND
selects + scrolls to the file (reusing the existing `pendingSelectPath` + scroll-into-view). Both
dialogs then jump straight to the matched file. Nightshift research loop 12.

## Acceptance Criteria
- [x] `revealFileInApp(path)` navigates to the parent folder and selects + scrolls the target file.
- [x] Content-search + duplicate hits dispatch the FILE path and land on the selected file.
- [x] Component tests updated to assert the file path is dispatched; npm check clean; suite green.

## Work Log
2026-07-15 - Nightshift loop 12. Reuse pendingSelectPath + the reactive scroll-into-view.

2026-07-15 - Done. Added `revealFileInApp(path)` in App.svelte (sets pendingSelectPath, navigates to the parent; the existing post-load hook selects it and the reactive block scrolls it into view). ContentSearchDialog + DuplicatesDialog now dispatch the FILE path; App's on:navigate handlers call revealFileInApp. Removed the now-unused parentDir import from ContentSearchDialog. Tests updated to assert the file path. npm check 0/0, npm test 411.

## Resolution
Reusable reveal helper; content-search and duplicate hits now land ON the file (selected + scrolled), not just its folder. Reuses existing selection/scroll machinery so no new rendering logic.
