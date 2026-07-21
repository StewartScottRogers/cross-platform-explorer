---
id: CPE-847
title: Structured-data reader — schema + paged rows (SQLite / Parquet / Excel)
type: feature
component: Backend
priority: medium
status: Open
tags: ready
created: 2026-07-21
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
- [ ] `sources(path)` lists SQLite tables/views and Excel/ODS sheets; Parquet returns its single source.
- [ ] `page(path, source, offset, limit)` returns typed columns + the requested row window + a total (or
      best-effort total) for each of SQLite, Parquet, Excel/ODS.
- [ ] Rows page correctly (offset/limit honoured); an out-of-range offset yields an empty window, not an
      error.
- [ ] Errors (missing file, bad source, unreadable) are `Err(String)`, never a panic.
- [ ] cargo-tested for all three formats (author a tiny fixture per format in the test); `cargo clippy
      --all-targets -D warnings` clean. No new dependency. App untouched.

## Work Log
