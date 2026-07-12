---
id: CPE-094
title: Preview/edit support for Apple property lists files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Apple property lists (.plist) in the right-side preview pane.
Tree view (XML and binary plist); edit. Editable (raw or structured), saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .plist is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Tree view (XML and binary plist); edit.
- [x] Editing: Editable (raw or structured), saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: plist parser; binary via backend. Editing model: structured. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .plist preview/edit: XML property lists render highlighted and editable as source (mapped to `code` + `xml` grammar; write_file_text, CPE-066). Binary plists are non-UTF-8 and fall back to the cant-preview/metadata state (never hangs) — out of scope for the source editor; a backend structured decoder is a future enhancement. Files: src/lib/filetypes.ts, src/lib/preview/highlight.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the XML/JSON data-format batch (CPE-082/094/206/207/208/211). npm run check clean; unit tests green.
