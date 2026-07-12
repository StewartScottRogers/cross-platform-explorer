---
id: CPE-219
title: Preview/edit support for DICOM medical images files
type: Feature
status: Open
priority: Low
component: Multiple
estimate: 4h+
created: 2026-07-11
closed:
---

## Summary

Add a preview provider for DICOM medical images (.dcm) in the right-side preview pane. Image plus key tags. Read-only viewer.

## Acceptance Criteria

- [ ] .dcm is matched by a dedicated preview provider in the bundled registry
- [ ] Viewer: Image plus key tags.
- [ ] Editing: Read-only viewer.
- [ ] Backend support: backend DICOM decode — verified locally (cargo) and green via CI.
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: backend DICOM decode. Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.