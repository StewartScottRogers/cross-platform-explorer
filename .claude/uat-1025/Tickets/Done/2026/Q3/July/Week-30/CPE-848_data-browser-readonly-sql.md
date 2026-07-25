---
id: CPE-848
title: Read-only SQL console for SQLite (SELECT-only, paged)
type: feature
component: Backend
priority: medium
status: Done
tags: needs-prereq
created: 2026-07-21
closed: 2026-07-21
epic: CPE-721
estimate: 2-3h
---

## Summary
Second child of CPE-721. A read-only SQL entry point for SQLite in `cpe-server::data_browser`: run a
user SELECT and return the same `{ columns, rows, total }` shape as CPE-847's `page`, so the grid renders
query results identically. **Safety:** open the database read-only and reject any non-read statement
(writes/DDL/PRAGMA-with-side-effects) before executing — the console can never mutate the file.

## Acceptance Criteria
- [x] `query(path, sql, offset, limit)` runs a read-only SELECT and returns typed columns + a paged row
      window; multiple statements / writes / DDL are refused with a clear error.
- [x] The database is opened read-only (a write attempt fails even if the guard were bypassed).
- [x] cargo-tested: a SELECT returns rows; an INSERT/UPDATE/DROP is rejected; `clippy --all-targets -D
      warnings` clean.

## Notes
Prereq: **CPE-847** (shares the `{columns, rows, total}` model).

## Work Log

## Resolution
Added `data_browser::query(path, sql, offset, limit)` — a **read-only** SQL entry point returning the same
`Page {columns, rows, total}` as CPE-847's `page`, so the grid renders query results identically.

Safety (two layers): the connection is opened `SQLITE_OPEN_READ_ONLY` (a write fails at the DB), and a
guard refuses anything but a **single** `SELECT`/`WITH` statement (first-token check + a multi-statement
`;` check) before execution. Paging is lazy — `skip(offset).take(limit)` on the row cursor — so a large
result set isn't loaded fully; `total` is `None` (unknown without exhausting the query).

Files: `crates/server/src/data_browser.rs` (+`query` + 1 test). No new dependency.

Verification (local, Windows): a `SELECT`/`WITH` returns typed columns + a paged window; `INSERT`/`UPDATE`/
`DROP`/multi-statement are refused; the DB is verified unchanged after the rejected writes. `cargo test`
data_browser → 5 passed; `cargo clippy --all-targets -D warnings` clean. CPE-849 (the grid UI) wires this.

## Work Log
- 2026-07-21 — Added the read-only SQL console reusing the Page model; two-layer safety (RO connection +
  single-SELECT guard), lazy paging. Tests + clippy clean. Closing.
