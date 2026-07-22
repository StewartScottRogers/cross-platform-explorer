---
id: CPE-190
title: Preview/edit support for BibTeX bibliographies files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for BibTeX bibliographies (.bib) in the right-side preview pane. Highlighted BibTeX. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .bib is matched by a dedicated preview provider in the bundled registry
- [x] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [x] Viewer: Highlighted BibTeX.
- [x] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js grammar: bibtex. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered .bib preview/edit: mapped to `code`; editable as source (write_file_text, CPE-066). No BibTeX grammar ships with highlight.js in this build, so escaped-monospace fallback per the AC (rather than a wrong-language alias). Cancellation + fallback from the shared PreviewPane. Files: src/lib/filetypes.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the markup/doc format batch (CPE-073/074/075/076/188/189/190). npm run check clean; unit tests green (incl. LaTeX/AsciiDoc grammar render tests).
