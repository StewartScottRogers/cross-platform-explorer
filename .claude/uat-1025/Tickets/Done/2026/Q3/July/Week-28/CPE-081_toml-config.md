---
id: CPE-081
title: Preview/edit support for TOML files
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for TOML (.toml) in the right-side preview pane.
Syntax-highlighted with parse validation; edit the source. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .toml is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Syntax-highlighted with parse validation; edit the source.
- [x] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: TOML parser. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .toml preview/edit as highlighted, editable source. Already mapped to `code` + the `ini` grammar (TOML is INI-like) by prior work; verified and regression-tested here. Edit/save via write_file_text (CPE-066); cancellation + fallback from the shared PreviewPane.

## Work Log

2026-07-12 — Implemented and closed as part of the config formats batch (CPE-080/081/191/192/193/199). npm run check clean; unit tests green.
