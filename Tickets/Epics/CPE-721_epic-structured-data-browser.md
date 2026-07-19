---
id: CPE-721
title: "EPIC: Structured-data browser (SQLite / Parquet / Excel)"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

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
