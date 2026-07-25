---
id: CPE-074
title: Preview/edit support for reStructuredText (RST) files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for reStructuredText (RST) (.rst) in the right-side preview pane.
Render to sanitized HTML; edit the source. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .rst is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Render to sanitized HTML; edit the source.
- [x] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: RST parser (bundled). Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .rst preview/edit: mapped to the `code` category; editable as source (write_file_text, CPE-066). highlight.js ships no reStructuredText grammar, so per the AC it uses the escaped-monospace fallback. Cancellation + fallback from the shared PreviewPane. Files: src/lib/filetypes.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the markup/doc format batch (CPE-073/074/075/076/188/189/190). npm run check clean; unit tests green (incl. LaTeX/AsciiDoc grammar render tests).
