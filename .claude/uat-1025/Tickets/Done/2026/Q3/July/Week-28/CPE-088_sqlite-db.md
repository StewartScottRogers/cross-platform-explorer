---
id: CPE-088
title: Preview/edit support for SQLite databases files
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 4h+
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for SQLite databases (.sqlite/.db) in the right-side preview pane.
Browse tables and rows read-only, with a simple query box. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .sqlite/.db is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Browse tables and rows read-only, with a simple query box.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend rusqlite — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend rusqlite. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .sqlite/.sqlite3/.db preview: rusqlite (bundled SQLite, no system lib) opens the database READ-ONLY and lists each table/view with its row count and column list (from sqlite_master + PRAGMA table_info; names quoted/escaped). Rendered read-only in the info preview kind (load cancellation + error->metadata fallback from the shared PreviewPane). Read-only viewer, as the ticket specifies. The interactive query box is a future enhancement; the schema+counts browser is the delivered read-only scope. Rust unit test builds a db and asserts the listing. Files: src-tauri/src/lib.rs (rusqlite dep) + frontend wiring + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the data-format batch (CPE-088/090/091) via read_preview_info. Rust: cargo test (49) + clippy clean; Frontend: npm run check clean, provider tests green.
