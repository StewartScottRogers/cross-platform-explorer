---
id: CPE-177
title: Preview/edit support for Sass files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for Sass (.sass) in the right-side preview pane. Highlighted Sass. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .sass is matched by a dedicated preview provider in the bundled registry
- [ ] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [ ] Viewer: Highlighted Sass.
- [ ] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js grammar: scss. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.

## Resolution

Mapped .sass to the scss highlight grammar and added it as a code extension, so Sass files preview highlighted and are editable. check + suite green.
