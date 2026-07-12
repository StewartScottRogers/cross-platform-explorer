---
id: CPE-214
title: Preview/edit support for Generic binary (hex viewer) files
type: Feature
status: Open
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a preview provider for Generic binary (hex viewer) (.bin/.dat) in the right-side preview pane. Hex + ASCII dump. Read-only viewer.

## Acceptance Criteria

- [ ] .bin/.dat is matched by a dedicated preview provider in the bundled registry
- [ ] Viewer: Hex + ASCII dump.
- [ ] Editing: Read-only viewer.
- [ ] Backend support: backend byte read — verified locally (cargo) and green via CI.
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: backend byte read. Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.