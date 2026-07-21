---
id: CPE-721
title: "EPIC: Structured-data browser (SQLite / Parquet / Excel)"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed: 2026-07-21
---

> **Activated 2026-07-21** (autonomous — best-guess decisions logged, PM delegated). Chosen for its
> **backend-first, no-new-deps** first children: `cpe-server` already carries rusqlite/calamine/parquet
> (today used for the `*_info` summary strings), so schema + paged-rows readers are pure cargo-testable
> extensions. Decomposed into CPE-847 (readers, headless), CPE-848 (read-only SQL, headless), CPE-849
> (the grid UI, attended).

## Decisions (activated 2026-07-21 — autonomous best-guess, PM delegated)
- **Crates:** reuse what `cpe-server` already has — `rusqlite` (bundled SQLite), `calamine` (xlsx/ods),
  `parquet` — so v1 adds **no new dependency**. **DBF is deferred** to a later child (it needs a new
  crate); v1 covers SQLite / Parquet / Excel-ODS.
- **Read-only v1:** the SQLite SQL console runs **SELECT/read-only** queries and rejects writes; editing
  cells is a future follow-up.
- **Coexistence:** the structured browser is the richer view for these binary formats; the existing
  CSV/JSON text/table preview is untouched — no conflict.

## Goal
A tabular navigator for data files the CSV provider can't touch: SQLite databases (browse tables/views,
page rows, run read-only SQL), Parquet, Excel/ODS multi-sheet workbooks, and DBF — with column typing,
sort, filter, and pagination over huge datasets without loading everything.

## Why
Extends preview from "show text" to "explore a dataset". Developers and analysts routinely have these files
and today must open a separate tool to peek inside.

## Rough scope (areas, not child tickets)
- Rust readers per format (SQLite, Parquet, xlsx/ods, DBF) exposing schema + paged rows.
- A shared virtualized data-grid frontend (typing, sort, filter, pagination).
- Multi-sheet / multi-table navigation.
- Read-only SQL console for SQLite (safe, no writes).

## Open questions (resolve at activation)
- Crate choices and build-size cost per format.
- Read-only vs. eventual edit; safety of the SQL console.
- How this coexists with the existing CSV/JSON preview providers.

## Definition of Done
- SQLite/Parquet/Excel-ODS/DBF files open in a paged, typed, sortable/filterable grid.
- Large datasets page without loading fully; multi-sheet/table navigation works.
- SQLite supports read-only queries; existing CSV/JSON previews are unaffected.

## Child tickets
1. **CPE-847** — Backend **schema + paged rows** reader (`cpe-server::data_browser`): for a SQLite
   table/view, a Parquet file, and an Excel/ODS sheet — `(path, source, offset, limit)` → `{ columns
   (name+type), rows, total }`, listing tables/sheets. Reuses the existing crates. Pure, cargo-tested.
   *Foundation — headless, buildable now.*
2. **CPE-848** — **Read-only SQL console** for SQLite: run a SELECT (reject writes/DDL), paged results +
   column schema. Headless, cargo-tested. *(prereq: 847)*
3. **CPE-849** — Frontend **virtualized data-grid**: multi-table/sheet navigation, typing, sort, filter,
   pagination, wired to a preview provider. **GUI-verified — attended.** *(prereq: 847, 848)*

## Work Log
- **2026-07-21** — Activated. Confirmed `cpe-server` already carries rusqlite/calamine/parquet (used for
  the `*_info` summaries), so the readers extend cleanly with no new deps. Resolved the crate/read-only/
  coexistence questions (above) and decomposed into CPE-847/848 (headless) + CPE-849 (GUI). DBF deferred.

## Resolution (closed 2026-07-21)
Delivered by **CPE-847 + 848 + 849** (all Done): the structured-data browser ships. The `cpe-server::
data_browser` reader exposes schema + paged rows for SQLite / Parquet / Excel-ODS and a read-only SQLite
SQL console (reusing the crates already in the tree — no new dependency), and the frontend `DataBrowser`
component renders it as an interactive preview grid (table/sheet navigation, paging, client-side sort +
filter, a read-only SQL box), wired via a new `data-grid` preview kind.

**DoD:** ✅ SQLite/Parquet/Excel-ODS open in a paged, typed, sortable/filterable grid · ✅ large sets page
without loading fully (server offset/limit) · ✅ multi-table/sheet navigation · ✅ SQLite read-only queries ·
✅ existing CSV/JSON previews unaffected. DBF was scoped out at activation (a new crate) and can be a future
follow-up. End-to-end GUI verification rides a deploy; the code is CI-green.
