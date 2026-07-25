---
id: CPE-425
title: "Save file list to a CSV/TXT file"
type: Feature
status: Done
priority: Low
component: Multiple
tags: [ready]
estimate: 45m
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Complements Copy-file-list (CPE-422): Tools > "Save file list..." exports the visible entries to a
file via a native Save dialog. `.csv` gets a CSV manifest (Name,Size-bytes,Modified-ISO, properly
escaped); `.txt` gets the Name/Size table. Nightshift research loop 14.

## Acceptance Criteria
- [x] `csvList(entries)` is pure + unit-tested (cell escaping, byte sizes, UTC-ISO modified, blank
      folder size / no-date).
- [x] Tools menu "Save file list..." opens a Save dialog (csv/txt filters) and writes via
      `write_file_text`; extension chooses CSV vs the tab table; no-op on Home/empty.
- [x] npm run check clean; JS suite green. (`dialog:default` already covers `save`.)

## Work Log
2026-07-15 - Nightshift loop 14. `csvList` in listing.ts (+test); imported `save` from
plugin-dialog; App `saveFileList()` -> save dialog -> write_file_text(csvList/detailList). Verified:
listing 3 tests, npm run check 0/0, npm test 415 passed. GUI (the native dialog) not driven.

## Resolution
Reuses the tested formatters + existing write_file_text; CSV is deterministic (UTC ISO) so it's
unit-testable. No capability change needed - `dialog:default` includes save.
