---
id: CPE-099
title: Preview/edit support for TIFF images files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for TIFF images (.tiff/.tif) in the right-side preview pane.
Multi-page TIFF viewer. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .tiff/.tif is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Multi-page TIFF viewer.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend/utif decode — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend/utif decode. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .tiff/.tif preview: the webview cannot decode TIFF natively, so a new backend command read_image_data_url decodes it with the image crate and returns a PNG data: URL. A new "decoded-image" preview provider (registered BEFORE the native image provider so it wins for TIFF) shows it via <img>, with load cancellation + error state. Rust test round-trips a real TIFF through the transcoder. Files: src-tauri/src/lib.rs (image dep), src/lib/preview/provider.ts, PreviewPane.svelte, App.svelte + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the parquet/image-transcode/7z batch (CPE-089/099/101/110). Rust: cargo test (53) + clippy clean; Frontend: npm run check clean, full vitest suite green (225).
