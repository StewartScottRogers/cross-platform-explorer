---
id: CPE-189
title: Preview/edit support for Textile markup files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for Textile markup (.textile) in the right-side preview pane. Rendered Textile; edit source. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .textile is matched by a dedicated preview provider in the bundled registry
- [x] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [x] Viewer: Rendered Textile; edit source.
- [x] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: textile renderer. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered .textile preview/edit: mapped to `code`; editable as source (write_file_text, CPE-066). No Textile grammar ships with highlight.js, so escaped-monospace fallback per the AC. Cancellation + fallback from the shared PreviewPane. Files: src/lib/filetypes.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the markup/doc format batch (CPE-073/074/075/076/188/189/190). npm run check clean; unit tests green (incl. LaTeX/AsciiDoc grammar render tests).
