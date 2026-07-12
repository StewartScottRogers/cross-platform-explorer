---
id: CPE-099
title: Preview/edit support for TIFF images files
type: Feature
status: Open
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for TIFF images (.tiff/.tif) in the right-side preview pane.
Multi-page TIFF viewer. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .tiff/.tif is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Multi-page TIFF viewer.
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Backend support: Backend/utif decode — lands green via CI (Rust builds/tests locally now too)
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend/utif decode. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.
