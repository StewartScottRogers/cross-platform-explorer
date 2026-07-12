---
id: CPE-075
title: Preview/edit support for AsciiDoc files
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for AsciiDoc (.adoc) in the right-side preview pane.
Render to sanitized HTML; edit the source. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .adoc is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Render to sanitized HTML; edit the source.
- [ ] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: asciidoctor.js (bundle-size review). Editing model: source. Editable types reuse [[CPE-066]] write_file_text.
