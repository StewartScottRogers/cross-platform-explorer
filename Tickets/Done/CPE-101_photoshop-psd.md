---
id: CPE-101
title: Preview/edit support for Photoshop documents (PSD) files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 4h+
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Photoshop documents (PSD) (.psd) in the right-side preview pane.
Flattened composite preview. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .psd is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Flattened composite preview.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend PSD parse — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend PSD parse. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .psd preview: read_image_data_url flattens the PSD to its RGBA composite via the psd crate and encodes a PNG data: URL, shown through the same "decoded-image" provider as TIFF. Layer-by-layer inspection is a future enhancement; the flattened composite is the delivered visual preview. Rust test covers the corrupt-file error path. Files: src-tauri/src/lib.rs (psd dep) + frontend wiring + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the parquet/image-transcode/7z batch (CPE-089/099/101/110). Rust: cargo test (53) + clippy clean; Frontend: npm run check clean, full vitest suite green (225).
