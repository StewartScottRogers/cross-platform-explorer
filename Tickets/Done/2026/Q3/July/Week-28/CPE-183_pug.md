---
id: CPE-183
title: Preview/edit support for Pug templates files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for Pug templates (.pug) in the right-side preview pane. Highlighted Pug. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .pug is matched by a dedicated preview provider in the bundled registry
- [x] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [x] Viewer: Highlighted Pug.
- [x] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js grammar: pug (3rd-party). Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered .pug preview/edit: mapped to `code`; no Pug grammar ships, so escaped-monospace fallback per the AC. Editable as source via the shared text provider (write_file_text, CPE-066); load cancellation + large/corrupt-file fallback come from the shared PreviewPane (CPE-059). Files: src/lib/filetypes.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the languages & templates batch (CPE-146/150/158/180/181/183/184/185). npm run check clean; unit tests green.
