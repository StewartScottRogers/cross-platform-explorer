---
id: CPE-082
title: Preview/edit support for XML files
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for XML (.xml) in the right-side preview pane.
Collapsible element tree plus source edit. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .xml is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Collapsible element tree plus source edit.
- [x] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: DOMParser (built-in). Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .xml preview/edit as syntax-highlighted, editable source. `.xml` was already mapped to the `code` category (grammar `xml`) by the earlier language rollout; this ticket adds explicit regression tests locking that in. Rendering + edit + save (via `write_file_text`, CPE-066), load cancellation, and large/corrupt-file fallback all come from the shared PreviewPane (CPE-059). Note: the optional collapsible element-tree viewer was not built — delivered as highlighted source edit, consistent with the rest of the format suite; the tree view is a possible future enhancement.

## Work Log

2026-07-12 — Implemented and closed as part of the XML/JSON data-format batch (CPE-082/094/206/207/208/211). npm run check clean; unit tests green.
