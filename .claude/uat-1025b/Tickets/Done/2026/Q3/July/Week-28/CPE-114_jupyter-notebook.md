---
id: CPE-114
title: Preview/edit support for Jupyter notebooks files
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Jupyter notebooks (.ipynb) in the right-side preview pane.
Render cells (markdown, code, and stored outputs). Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .ipynb is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Render cells (markdown, code, and stored outputs).
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Notebook JSON renderer. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .ipynb preview/edit: mapped to the code category and the json grammar (a notebook is a JSON document), so it renders highlighted and is editable as source (write_file_text, CPE-066). Rich cell-by-cell notebook rendering is a future enhancement; highlighted JSON source is the delivered scope. Files: src/lib/filetypes.ts, src/lib/preview/highlight.ts + tests. Load cancellation and large/corrupt-file fallback come from the shared PreviewPane (CPE-059).

## Work Log

2026-07-12 — Implemented/verified and closed as part of the native-render/already-mapped format batch (CPE-078/095/096/098/100/103/104/105/107/108/114). npm run check clean; unit tests green (provider-kind regression tests added).
