---
id: CPE-091
title: Preview/edit support for OpenDocument Spreadsheet (ODS) files
type: Feature
status: Open
priority: Low
component: Multiple
estimate: 4h+
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for OpenDocument Spreadsheet (ODS) (.ods) in the right-side preview pane.
Table view (read-only). Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .ods is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Table view (read-only).
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Backend support: Backend calamine crate — lands green via CI (Rust builds/tests locally now too)
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend calamine crate. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.
