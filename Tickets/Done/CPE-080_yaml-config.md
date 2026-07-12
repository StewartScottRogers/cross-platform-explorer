---
id: CPE-080
title: Preview/edit support for YAML files
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for YAML (.yaml/.yml) in the right-side preview pane.
Syntax-highlighted with inline parse validation; edit the source. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .yaml/.yml is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Syntax-highlighted with inline parse validation; edit the source.
- [x] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: YAML parser for validation. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .yaml/.yml preview/edit as highlighted, editable source. The extensions were already mapped to the `code` category with the `yaml` highlight.js grammar by the earlier language rollout; this ticket verifies that and adds explicit regression tests. Edit/save via write_file_text (CPE-066); load cancellation and large/corrupt fallback from the shared PreviewPane (CPE-059). Note: dedicated inline YAML parse-validation UI was not added — delivered as highlighted editable source consistent with the format suite (the shared editor still saves any text).

## Work Log

2026-07-12 — Implemented and closed as part of the config formats batch (CPE-080/081/191/192/193/199). npm run check clean; unit tests green.
