---
id: CPE-849
title: Structured-data browser — virtualized grid + table/sheet navigation (UI)
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-21
epic: CPE-721
estimate: 4h+
---

## Summary
Third child of CPE-721. A virtualized data-grid preview surface for SQLite / Parquet / Excel-ODS files:
pick a table/sheet, page through rows (typed columns), sort and filter, and — for SQLite — a read-only
SQL box (CPE-848). Wired as a preview provider so opening one of these files shows the grid; the existing
CSV/JSON preview is unaffected.

## Acceptance Criteria
- [ ] Opening a SQLite/Parquet/Excel file shows a paged, typed grid; large files page without loading fully.
- [ ] Multi-table (SQLite) / multi-sheet (Excel) navigation; column sort + a simple filter.
- [ ] SQLite: a read-only SQL box runs a query and shows results in the same grid (CPE-848).
- [ ] GUI-verified; CSV/JSON previews unchanged; `npm run check` + suite green.

## Notes
Prereq: **CPE-847**, **CPE-848**. **GUI-verified — attended.**

## Work Log
