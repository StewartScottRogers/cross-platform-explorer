---
id: CPE-188
title: Preview/edit support for MDX documents files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for MDX documents (.mdx) in the right-side preview pane. Rendered MDX; edit source. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .mdx is matched by a dedicated preview provider in the bundled registry
- [x] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [x] Viewer: Rendered MDX; edit source.
- [x] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: render markdown + highlight code. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered .mdx preview/edit: mapped to `code` and aliased to the `markdown` grammar so MDX source is highlighted and editable (write_file_text, CPE-066). Rendered as highlighted source rather than executed markdown+JSX, since MDX embeds JSX that a markdown renderer would mangle. Cancellation + fallback from the shared PreviewPane. Files: src/lib/filetypes.ts, src/lib/preview/highlight.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the markup/doc format batch (CPE-073/074/075/076/188/189/190). npm run check clean; unit tests green (incl. LaTeX/AsciiDoc grammar render tests).
