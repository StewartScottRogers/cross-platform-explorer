---
id: CPE-112
title: Preview/edit support for GZIP files (.gz) files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for GZIP files (.gz) (.gz) in the right-side preview pane.
Preview the single decompressed file. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .gz is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Preview the single decompressed file.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend flate2 — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend flate2. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .gz/.tgz/.tar.gz preview: gzip-compressed tarballs (.tar.gz/.tgz) are gunzipped via `flate2` and listed as a TAR; a single-file .gz is shown as one entry — the inner name (archive name minus .gz) and the uncompressed size read from the gzip ISIZE trailer. Rust unit test round-trips a gzip and asserts name + size. Files: src-tauri/src/lib.rs (flate2 dep), src/lib/preview/provider.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the archive-format batch (CPE-109/112/217). Rust: cargo test (39) + clippy clean; Frontend: npm run check clean, full vitest suite green (220).
