---
id: CPE-071
title: Preview/edit support for Word documents (DOCX) files
type: Feature
status: Open
priority: Medium
component: Multiple
estimate: 4h+
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for Word documents (DOCX) (.docx) in the right-side preview pane.
Render the document to sanitized HTML for reading. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .docx is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Render the document to sanitized HTML for reading.
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Backend support: mammoth.js in-browser, or a backend docx parse — lands green via CI (Rust builds/tests locally now too)
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: mammoth.js in-browser, or a backend docx parse. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.
