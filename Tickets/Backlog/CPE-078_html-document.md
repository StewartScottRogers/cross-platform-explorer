---
id: CPE-078
title: Preview/edit support for HTML documents files
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for HTML documents (.html) in the right-side preview pane.
Rendered in a sandboxed iframe (no script/network) with a source-edit toggle. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .html is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Rendered in a sandboxed iframe (no script/network) with a source-edit toggle.
- [ ] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: srcdoc sandbox iframe. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.
