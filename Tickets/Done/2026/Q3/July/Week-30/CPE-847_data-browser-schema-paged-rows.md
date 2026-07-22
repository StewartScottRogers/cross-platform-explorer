---
id: CPE-847
title: Structured-data reader — schema + paged rows (SQLite / Parquet / Excel)
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-721
estimate: 3-4h
---

## Summary
Foundation for the structured-data browser (CPE-721). A pure `cpe-server::data_browser` module that reads
**schema + a page of rows** from a data file, reusing the crates already in `cpe-server` (rusqlite,
calamine, parquet) — no new dependency:

- **list sources** — the table/view names (SQLite), the sheet names (Excel/ODS); Parquet is a single
  source.
- **read page** — `(path, source, offset, limit)` → `{ columns: [{name, type}], rows: [[cell, …]], total }`
  where cells are stringified for a uniform grid; large files page without loading fully.

## Acceptance Criteria
- [x] `sources(path)` lists SQLite tables/views (name-sorted) and Excel/ODS sheets; Parquet returns an
      empty list (single source).
- [x] `page(path, source, offset, limit)` returns typed columns + the requested row window + a total for
      each of SQLite, Parquet, Excel/ODS.
- [x] Rows page correctly (offset/limit honoured); an out-of-range offset yields an empty window, not an
      error.
- [x] Errors (missing file, bad source, unsupported ext) are `Err(String)`, never a panic.
- [x] cargo-tested for all three formats (SQLite via rusqlite, xlsx via rust_xlsxwriter, parquet via the
      low-level writer); `cargo clippy --all-targets -D warnings` clean. No new dependency. App untouched.

## Resolution
Added **`cpe-server::data_browser`** — the reader behind the structured-data grid:

- `Column { name, type }` + `Page { columns, rows, total }` (serde) — cells stringified for a uniform grid.
- `sources(path)` / `page(path, source, offset, limit)` dispatch by extension (`detect`): `.db/.sqlite*`,
  `.parquet`, `.xlsx/.xlsm/.xlsb/.xls/.ods`; an unknown extension is an error.
- **SQLite** — opened `SQLITE_OPEN_READ_ONLY`; sources from `sqlite_master`; columns+types from
  `PRAGMA table_info`; a `LIMIT/OFFSET SELECT *` window; `COUNT(*)` total; identifiers quoted; cells via
  `ValueRef` (NULL→"", blob→`<N bytes>`).
- **Spreadsheet** — calamine; sheets from `sheet_names`; the first row is the header (columns), remaining
  rows paged; total = height − 1; empty source defaults to the first sheet.
- **Parquet** — schema/columns + `num_rows` total from the footer; rows via the row iterator
  `skip(offset).take(limit)`.

Files: `crates/server/src/data_browser.rs` (impl + 4 tests), registered in `lib.rs`. **No new dependency**
(reuses rusqlite/calamine/parquet already present for the `*_info` summaries).

Verification (local, Windows): `cargo test` → **152 passed** (was 148; +4 covering all three formats,
paging, out-of-range, and the unsupported-extension error); `cargo clippy --all-targets -D warnings` clean.
App untouched. CPE-848 (read-only SQL) reuses this `Page` model; CPE-849 is the grid UI.

## Work Log
- 2026-07-21 — Picked up (epic CPE-721 activation). Estimate 3-4h. Built `data_browser` reusing the
  existing data crates: schema + paged rows for SQLite/Parquet/Excel, dispatched by extension. Fixed a
  `&&str` test typo. 4 tests (incl. a hand-authored parquet fixture) + full suite 152 green; clippy clean.
  Closing.
