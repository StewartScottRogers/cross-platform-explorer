---
id: CPE-199
title: Preview/edit support for EditorConfig files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for EditorConfig (.editorconfig) in the right-side preview pane. Highlighted EditorConfig. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .editorconfig is matched by a dedicated preview provider in the bundled registry
- [x] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [x] Viewer: Highlighted EditorConfig.
- [x] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js grammar: ini. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered .editorconfig preview/edit as highlighted, editable source. `.editorconfig` is matched by name via CODE_FILENAMES (category `code`) and resolves to the `ini` grammar via LANG_BY_FILENAME — already wired by prior work; verified and regression-tested here (categoryOf + languageForName). Edit/save via write_file_text; cancellation + fallback from the shared PreviewPane.

## Work Log

2026-07-12 — Implemented and closed as part of the config formats batch (CPE-080/081/191/192/193/199). npm run check clean; unit tests green.
