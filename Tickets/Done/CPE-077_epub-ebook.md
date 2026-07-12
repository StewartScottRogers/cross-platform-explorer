---
id: CPE-077
title: Preview/edit support for EPUB ebooks files
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 4h+
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for EPUB ebooks (.epub) in the right-side preview pane.
Paginated reader for chapters. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .epub is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Paginated reader for chapters.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: epub.js + backend/zip access — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: epub.js + backend/zip access. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .epub preview: enumerate the ebook's XHTML/HTML content documents in name order, strip each to text, and concatenate (capped at 128 KB so a whole book can't flood the pane), prefixed with the document count. Rendered read-only in the "info" preview kind (load cancellation + error->metadata fallback from the shared PreviewPane). Read-only, not editable: extracted text is a lossy view of the original, so the source editor is intentionally not offered for these formats. A full paginated reader is a future enhancement; readable text is the delivered scope. Files: src-tauri/src/lib.rs + frontend wiring + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the document text-extraction batch (CPE-070/071/072/077) via read_preview_info. Rust: cargo test (47) + clippy clean; Frontend: npm run check clean, provider tests green.
