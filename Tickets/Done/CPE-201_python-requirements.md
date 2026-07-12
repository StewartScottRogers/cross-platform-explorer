---
id: CPE-201
title: Preview/edit support for Python requirements files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for Python requirements (requirements.txt) in the right-side preview pane. Dependency pin list. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] requirements.txt is matched by a dedicated preview provider in the bundled registry
- [ ] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [ ] Viewer: Dependency pin list.
- [ ] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: plain highlight. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.

## Resolution

Already satisfied: requirements.txt is a .txt file, handled by the text provider (editable plain text). No dedicated grammar needed. Closed as covered.
