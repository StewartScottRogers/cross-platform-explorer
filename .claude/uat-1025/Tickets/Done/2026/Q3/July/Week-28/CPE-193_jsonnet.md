---
id: CPE-193
title: Preview/edit support for Jsonnet files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for Jsonnet (.jsonnet/.libsonnet) in the right-side preview pane. Highlighted Jsonnet. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .jsonnet/.libsonnet is matched by a dedicated preview provider in the bundled registry
- [x] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [x] Viewer: Highlighted Jsonnet.
- [x] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js json-based. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered .jsonnet/.libsonnet preview/edit: mapped to `code` and aliased to the `json` grammar (Jsonnet is a JSON superset) for reasonable highlighting; editable as source (write_file_text, CPE-066). Cancellation + fallback from the shared PreviewPane. Files: src/lib/filetypes.ts, src/lib/preview/highlight.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the config formats batch (CPE-080/081/191/192/193/199). npm run check clean; unit tests green.
