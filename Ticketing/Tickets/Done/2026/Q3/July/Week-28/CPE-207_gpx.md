---
id: CPE-207
title: Preview/edit support for GPX GPS tracks files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for GPX GPS tracks (.gpx) in the right-side preview pane. Highlighted XML; optional track view. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .gpx is matched by a dedicated preview provider in the bundled registry
- [x] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [x] Viewer: Highlighted XML; optional track view.
- [x] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js xml. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered .gpx preview/edit: mapped to `code` + `xml` grammar; highlighted, editable XML source (write_file_text, CPE-066). Cancellation + fallback from the shared PreviewPane. Optional track-map view not added (optional). Files: src/lib/filetypes.ts, src/lib/preview/highlight.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the XML/JSON data-format batch (CPE-082/094/206/207/208/211). npm run check clean; unit tests green.
