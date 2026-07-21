---
id: CPE-848
title: Read-only SQL console for SQLite (SELECT-only, paged)
type: feature
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-21
epic: CPE-721
estimate: 2-3h
---

## Summary
Second child of CPE-721. A read-only SQL entry point for SQLite in `cpe-server::data_browser`: run a
user SELECT and return the same `{ columns, rows, total }` shape as CPE-847's `page`, so the grid renders
query results identically. **Safety:** open the database read-only and reject any non-read statement
(writes/DDL/PRAGMA-with-side-effects) before executing — the console can never mutate the file.

## Acceptance Criteria
- [ ] `query(path, sql, offset, limit)` runs a read-only SELECT and returns typed columns + a paged row
      window; multiple statements / writes / DDL are refused with a clear error.
- [ ] The database is opened read-only (a write attempt fails even if the guard were bypassed).
- [ ] cargo-tested: a SELECT returns rows; an INSERT/UPDATE/DROP is rejected; `clippy --all-targets -D
      warnings` clean.

## Notes
Prereq: **CPE-847** (shares the `{columns, rows, total}` model).

## Work Log
