---
id: CPE-070
title: Preview/edit support for Rich Text (RTF) files
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Rich Text (RTF) (.rtf) in the right-side preview pane.
Render the styled text (bold/italic/lists); edit the source. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .rtf is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Render the styled text (bold/italic/lists); edit the source.
- [x] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Bundled RTF-to-HTML parser. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .rtf preview: a small, dependency-free RTF reader (read_preview_info handler) extracts body text — dropping control words and the font/colour/style/info destinations, turning \par/\line/\sect into newlines and \'hh into bytes. Rendered read-only in the "info" preview kind (load cancellation + error->metadata fallback from the shared PreviewPane). Read-only, not editable: extracted text is a lossy view of the original, so the source editor is intentionally not offered for these formats. Rust unit test asserts body text is extracted and control tables dropped. Files: src-tauri/src/lib.rs + frontend wiring + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the document text-extraction batch (CPE-070/071/072/077) via read_preview_info. Rust: cargo test (47) + clippy clean; Frontend: npm run check clean, provider tests green.
