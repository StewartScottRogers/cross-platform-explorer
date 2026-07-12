---
id: CPE-116
title: Preview/edit support for Log files files
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for Log files (.log) in the right-side preview pane.
Level highlighting plus follow/tail for growing files. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .log is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Level highlighting plus follow/tail for growing files.
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: None. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.
