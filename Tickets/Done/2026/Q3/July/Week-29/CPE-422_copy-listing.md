---
id: CPE-422
title: "Copy folder listing to clipboard"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
estimate: 30m
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Export the current folder's visible entries to the clipboard as text via the Tools menu:
"Copy file names" (one name per line) and "Copy file list (name + size)" (a tab-separated table that
pastes into a spreadsheet/issue). Honours the active sort + filter (copies whatever is visible).
Non-destructive. Nightshift research loop 11.

## Acceptance Criteria
- [x] Tools menu gains "Copy file names" and "Copy file list (name + size)".
- [x] Copies the VISIBLE entries (respects sort/filter) via the clipboard; a notice confirms the count.
- [x] No-op with a friendly notice on Home / an empty view.
- [x] Pure formatters (`namesList`/`detailList`) unit-tested; npm check clean; suite green.

## Work Log
2026-07-15 - Nightshift loop 11. `src/lib/listing.ts` (namesList/detailList, reusing formatSize) +test;
two Tools-menu items routed in App.onMenuSelect -> copyListing() -> navigator.clipboard.writeText +
notice (same clipboard path as Copy-as-path). Verified: listing 2 tests, npm run check 0/0,
npm test 411 passed.

## Resolution
Small, safe, useful export. `detailList` is TSV (Name/Size) so it pastes as a table; `namesList` is a
bare name list. Date column omitted to keep the pure formatter timezone-deterministic for tests; can
be added later if wanted.
