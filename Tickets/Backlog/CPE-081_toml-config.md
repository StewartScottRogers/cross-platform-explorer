---
id: CPE-081
title: Preview/edit support for TOML files
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for TOML (.toml) in the right-side preview pane.
Syntax-highlighted with parse validation; edit the source. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .toml is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Syntax-highlighted with parse validation; edit the source.
- [ ] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: TOML parser. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.
