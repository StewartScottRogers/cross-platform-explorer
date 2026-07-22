---
id: CPE-110
title: Preview/edit support for 7-Zip archives (7z) files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for 7-Zip archives (7z) (.7z) in the right-side preview pane.
List entries read-only. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .7z is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: List entries read-only.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend sevenz crate — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend sevenz crate. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .7z preview: read_archive_entries now has a 7-Zip branch (sevenz-rust) listing members (name/size/is_dir), shown by the existing archive provider — added 7z to its extension set. Rust test covers the non-7z error path. Files: src-tauri/src/lib.rs (sevenz-rust dep), src/lib/preview/provider.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the parquet/image-transcode/7z batch (CPE-089/099/101/110). Rust: cargo test (53) + clippy clean; Frontend: npm run check clean, full vitest suite green (225).
