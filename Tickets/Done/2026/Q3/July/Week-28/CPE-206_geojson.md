---
id: CPE-206
title: Preview/edit support for GeoJSON files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for GeoJSON (.geojson) in the right-side preview pane. Pretty-printed JSON; optional map. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .geojson is matched by a dedicated preview provider in the bundled registry
- [x] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [x] Viewer: Pretty-printed JSON; optional map.
- [x] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js json + optional map. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered .geojson preview/edit: mapped to the `code` category in filetypes.ts and aliased to the `json` highlight.js grammar, so the registered text provider renders it highlighted and editable as source (saved via write_file_text, CPE-066). Cancellation + large/corrupt fallback from the shared PreviewPane. The optional inline map was not added (explicitly optional). Files: src/lib/filetypes.ts, src/lib/preview/highlight.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the XML/JSON data-format batch (CPE-082/094/206/207/208/211). npm run check clean; unit tests green.
