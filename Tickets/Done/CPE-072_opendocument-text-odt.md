---
id: CPE-072
title: Preview/edit support for OpenDocument Text (ODT) files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 4h+
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for OpenDocument Text (ODT) (.odt) in the right-side preview pane.
Render the document body for reading. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .odt is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Render the document body for reading.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend unzip + content.xml transform — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend unzip + content.xml transform. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .odt preview: unzip content.xml and strip the ODF text markup (text:p/text:h -> newlines) to plain text. Rendered read-only in the "info" preview kind (load cancellation + error->metadata fallback from the shared PreviewPane). Read-only, not editable: extracted text is a lossy view of the original, so the source editor is intentionally not offered for these formats. Reuses the same strip_markup_to_text + zip_read_text helpers as DOCX. Files: src-tauri/src/lib.rs + frontend wiring + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the document text-extraction batch (CPE-070/071/072/077) via read_preview_info. Rust: cargo test (47) + clippy clean; Frontend: npm run check clean, provider tests green.
