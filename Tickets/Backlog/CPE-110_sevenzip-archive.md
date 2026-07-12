---
id: CPE-110
title: Preview/edit support for 7-Zip archives (7z) files
type: Feature
status: Open
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for 7-Zip archives (7z) (.7z) in the right-side preview pane.
List entries read-only. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .7z is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: List entries read-only.
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Backend support: Backend sevenz crate — lands green via CI (Rust builds/tests locally now too)
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend sevenz crate. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.
