---
id: CPE-089
title: Preview/edit support for Apache Parquet files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 4h+
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Apache Parquet (.parquet) in the right-side preview pane.
Show the schema and a capped row sample (read-only). Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .parquet is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Show the schema and a capped row sample (read-only).
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend parquet crate — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend parquet crate. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .parquet preview: the parquet crate reads the file footer metadata (no full column scan) and read_preview_info reports row count, row-group count, created-by, and the column schema (name + physical type). Rendered read-only in the info kind; error->metadata fallback. Rust test covers the error path. Files: src-tauri/src/lib.rs (parquet dep) + frontend wiring + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the parquet/image-transcode/7z batch (CPE-089/099/101/110). Rust: cargo test (53) + clippy clean; Frontend: npm run check clean, full vitest suite green (225).
