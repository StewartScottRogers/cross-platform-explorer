---
id: CPE-094
title: Preview/edit support for Apple property lists files
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for Apple property lists (.plist) in the right-side preview pane.
Tree view (XML and binary plist); edit. Editable (raw or structured), saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .plist is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Tree view (XML and binary plist); edit.
- [ ] Editing: Editable (raw or structured), saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: plist parser; binary via backend. Editing model: structured. Editable types reuse [[CPE-066]] write_file_text.
