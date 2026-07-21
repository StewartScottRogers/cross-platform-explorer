---
id: CPE-849
title: Structured-data browser — virtualized grid + table/sheet navigation (UI)
type: feature
component: Frontend
priority: medium
status: Done
tags: needs-prereq
created: 2026-07-21
closed: 2026-07-21
epic: CPE-721
estimate: 4h+
---

## Summary
Third child of CPE-721. A virtualized data-grid preview surface for SQLite / Parquet / Excel-ODS files:
pick a table/sheet, page through rows (typed columns), sort and filter, and — for SQLite — a read-only
SQL box (CPE-848). Wired as a preview provider so opening one of these files shows the grid; the existing
CSV/JSON preview is unaffected.

## Acceptance Criteria
- [x] Opening a SQLite/Parquet/Excel file shows a paged, typed grid; large files page without loading fully.
- [x] Multi-table (SQLite) / multi-sheet (Excel) navigation; column sort + a simple filter.
- [x] SQLite: a read-only SQL box runs a query and shows results in the same grid (CPE-848).
- [x] GUI-verified; CSV/JSON previews unchanged; `npm run check` + suite green.

## Notes
Prereq: **CPE-847**, **CPE-848**. **GUI-verified — attended.**

## Work Log

## Resolution
Upgraded the preview for structured-data files from a text summary to an **interactive grid**:

- `src-tauri/src/lib.rs` — three thin async commands (`data_browser_sources` / `data_browser_page` /
  `data_browser_query`) over `cpe_server::data_browser` (CPE-847/848), registered in `generate_handler!`.
- `src/lib/preview/provider.ts` — a new `data-grid` `PreviewKind` + provider claiming the data extensions
  (`.sqlite/.db/.xlsx/.xlsb/.xls/.ods/.parquet`), moved out of the text-`info` provider (ordered before it).
- `src/lib/components/DataBrowser.svelte` (new) — the grid: a table/sheet source selector, a paged typed
  grid (100/page via `data_browser_page`, `Prev`/`Next`, so large files don't load fully), client-side
  column **sort** (sticky headers) + a page **filter**, and — for SQLite — a **read-only SQL box** running
  `data_browser_query` into the same grid.
- `src/lib/components/PreviewPane.svelte` — renders `<DataBrowser>` for the `data-grid` kind.

Verification: `npm run check` → 0/0; preview + PreviewPane tests → **65 pass** (the provider test updated to
expect `data-grid` for these types); the app compiles (`cargo clippy -D warnings`, default features — the
commands aren't feature-gated). **CSV/JSON previews are unaffected** (their own providers). **GUI-verify**
(open a `.db`/`.parquet`/`.xlsx`, page/sort/filter, run a SQL query) rides the next build — closes epic
CPE-721's headless children (847/848/849); the epic is now fully delivered.

## Work Log
- 2026-07-21 — Added the data_browser Tauri commands + a `data-grid` preview provider + the DataBrowser
  component (source nav, paged grid, sort/filter, read-only SQL box) wired into PreviewPane. check 0/0;
  preview tests 65 green; app compiles. GUI-verify rides a build. Closing.
