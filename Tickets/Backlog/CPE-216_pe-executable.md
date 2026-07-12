---
id: CPE-216
title: Preview/edit support for Windows PE (EXE/DLL) info files
type: Feature
status: Open
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a preview provider for Windows PE (EXE/DLL) info (.exe/.dll) in the right-side preview pane. Headers and imports. Read-only viewer.

## Acceptance Criteria

- [ ] .exe/.dll is matched by a dedicated preview provider in the bundled registry
- [ ] Viewer: Headers and imports.
- [ ] Editing: Read-only viewer.
- [ ] Backend support: backend PE parse — verified locally (cargo) and green via CI.
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: backend PE parse. Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.