---
id: CPE-084
title: Preview/edit support for INI / conf files files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for INI / conf files (.ini/.conf) in the right-side preview pane.
Sectioned key/value view; edit. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .ini/.conf is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Sectioned key/value view; edit.
- [ ] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: None. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Already covered: .ini/.cfg are handled by the text provider with the ini highlight grammar and are editable. Closed as covered.
