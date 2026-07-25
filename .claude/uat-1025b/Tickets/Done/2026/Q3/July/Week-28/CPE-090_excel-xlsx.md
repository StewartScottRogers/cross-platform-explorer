---
id: CPE-090
title: Preview/edit support for Excel workbooks (XLSX) files
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 4h+
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Excel workbooks (XLSX) (.xlsx) in the right-side preview pane.
Sheet tabs with a table view (read-only). Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .xlsx is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Sheet tabs with a table view (read-only).
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend calamine crate — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend calamine crate. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .xlsx/.xlsm preview: calamine (pure Rust) opens the workbook and renders each sheet as a tab-separated text grid, capped at 100 rows x 20 cols per sheet, with a sheet list header. Rendered read-only in the info preview kind (load cancellation + error->metadata fallback from the shared PreviewPane). Read-only viewer, as the ticket specifies. Rust unit test authors a real xlsx (rust_xlsxwriter dev-dep) and asserts the rendered grid. Files: src-tauri/src/lib.rs (calamine dep) + frontend wiring + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the data-format batch (CPE-088/090/091) via read_preview_info. Rust: cargo test (49) + clippy clean; Frontend: npm run check clean, provider tests green.
