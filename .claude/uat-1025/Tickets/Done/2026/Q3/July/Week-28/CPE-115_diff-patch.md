---
id: CPE-115
title: Preview/edit support for Diff / patch files files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Diff / patch files (.diff/.patch) in the right-side preview pane.
Coloured unified-diff view; edit the source. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .diff/.patch is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Coloured unified-diff view; edit the source.
- [ ] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: None. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Added .diff/.patch as code mapped to the highlight.js diff grammar, which colours +/- lines; editable as source.
