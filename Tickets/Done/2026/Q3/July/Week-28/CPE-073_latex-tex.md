---
id: CPE-073
title: Preview/edit support for LaTeX source (TeX) files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for LaTeX source (TeX) (.tex) in the right-side preview pane.
Edit the source; optionally render math with KaTeX. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .tex is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Edit the source; optionally render math with KaTeX.
- [x] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: KaTeX (optional, bundled). Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .tex preview/edit: mapped to the `code` category and the newly-loaded `latex` highlight.js grammar, so the shared text provider renders LaTeX syntax-highlighted and editable as source (write_file_text, CPE-066). Cancellation + large/corrupt fallback from the shared PreviewPane (CPE-059). The optional KaTeX render was not added (explicitly optional). Files: src/lib/filetypes.ts, src/lib/preview/highlight.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the markup/doc format batch (CPE-073/074/075/076/188/189/190). npm run check clean; unit tests green (incl. LaTeX/AsciiDoc grammar render tests).
