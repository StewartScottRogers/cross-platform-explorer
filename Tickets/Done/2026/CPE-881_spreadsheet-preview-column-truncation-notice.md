---
id: CPE-881
title: Note truncated columns in the spreadsheet preview (silent-truncation consistency)
type: bug
component: Server
priority: low
tags: ready
epic: CPE-706
created: 2026-07-21
closed: 2026-07-21
status: Done
---

## Summary
`data_preview::spreadsheet_info` caps the grid at `MAX_ROWS` and `MAX_COLS` and appends a "… N more rows"
notice when rows are cut — but said **nothing** when columns are cut. A sheet wider than 20 columns silently
rendered only the first 20 and looked complete: the same "looks complete but isn't" silent-truncation gap
just fixed for the hex dump (CPE-879).

Added the symmetric "… N more columns" notice.

## Acceptance Criteria
- [x] A sheet wider than `MAX_COLS` appends a "… N more columns" notice; a narrow sheet does not.
- [x] Existing row-truncation notice unchanged.
- [x] `cargo test` + `cargo clippy --all-targets -D warnings` green in `cpe-server`.

## Work Log
- 2026-07-21 (autonomous) — Spotted the asymmetry while auditing the preview providers for the same
  silent-truncation class as CPE-879. Added the column notice + a wide-sheet test. 4/4 data_preview tests
  pass; clippy clean.
