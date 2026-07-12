---
id: CPE-113
title: Preview/edit support for ISO disc images files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 4h+
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for ISO disc images (.iso) in the right-side preview pane.
List the disc contents. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .iso is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: List the disc contents.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend iso9660 parse — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend iso9660 parse. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .iso preview: read_archive_entries now has an ISO 9660 branch (iso9660 crate) that walks the disc image and lists its files (paths + sizes, directories flagged), bounded to 2000 entries so a huge image cannot flood the pane. Shown by the existing archive provider (iso added to its extension set). Rust test covers the non-ISO error path. Files: src-tauri/src/lib.rs (iso9660 dep), src/lib/preview/provider.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the ISO + font batch (CPE-113/117). Rust: cargo test (54) + clippy clean; Frontend: npm run check clean, full vitest suite green (226).
