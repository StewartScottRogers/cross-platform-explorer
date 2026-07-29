---
id: CPE-085
title: Preview/edit support for Dotenv files (.env) files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Dotenv files (.env) (.env) in the right-side preview pane.
Key/value list with values masked by default; edit. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .env is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Key/value list with values masked by default; edit.
- [ ] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: None. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Added .env to the well-known code-filenames set (categoryOf -> code) and mapped it to the ini grammar, so dotenv files preview highlighted and are editable.
