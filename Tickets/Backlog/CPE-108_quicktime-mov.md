---
id: CPE-108
title: Preview/edit support for QuickTime video (MOV) files
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for QuickTime video (MOV) (.mov) in the right-side preview pane.
Play via the video element. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .mov is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Play via the video element.
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: None. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.
