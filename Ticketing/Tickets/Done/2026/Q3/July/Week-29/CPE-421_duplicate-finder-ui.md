---
id: CPE-421
title: "Find duplicate files - results UI"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 1h
created: 2026-07-15
closed: 2026-07-15
---

## Summary
UI over the CPE-420 duplicate-finder engine: a Tools > "Find duplicate files..." overlay that scans
the current folder on demand, lists byte-identical groups (largest reclaimable space first) with a
total-reclaimable summary, and jumps to a file's folder on click. Read-only + safe - it never
deletes. Nightshift research loop 10; completes the duplicate-finder feature.

## Acceptance Criteria
- [x] Tools menu gains "Find duplicate files..." (and "Search in files...", Ctrl+Shift+F, for
      discoverability); both gated off Home/archive.
- [x] Overlay scans on an explicit button (not automatic), shows groups + reclaimable total + scanned
      count, truncated note; loading / empty / error states.
- [x] Clicking a file navigates to its parent folder; nothing is ever deleted.
- [x] Component test (mocked invoke: scan -> group -> click navigates; no-dup case); npm check 0/0; suite green.

## Work Log
2026-07-15 - Nightshift loop 10. Added a Tools menu (MenuBar) with Search-in-files + Find-duplicates,
routed in App.onMenuSelect (gated off Home/archive); `DuplicatesDialog.svelte` (+test) over
`find_duplicates`, reusing `contentSearch.ts` baseName/parentDir + `format.formatSize`. Verified:
DuplicatesDialog 2 tests, npm run check 0/0, npm test 409 passed. GUI not driven (machine-share rule).

## Resolution
Completes duplicate detection end-to-end (engine CPE-420 + this UI). Deliberately non-destructive
(navigate, not delete) for a Nightshift-built feature; a guarded "delete extra copies" action is a
possible later enhancement.
