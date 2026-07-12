---
id: CPE-217
title: Preview/edit support for JAR / APK archives files
type: Feature
status: Open
priority: Low
component: Multiple
estimate: 1h
created: 2026-07-11
closed:
---

## Summary

Add a preview provider for JAR / APK archives (.jar/.apk) in the right-side preview pane. Entry list. Read-only viewer.

## Acceptance Criteria

- [ ] .jar/.apk is matched by a dedicated preview provider in the bundled registry
- [ ] Viewer: Entry list.
- [ ] Editing: Read-only viewer.
- [ ] Backend support: reuse read_archive_entries (zip) — verified locally (cargo) and green via CI.
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: reuse read_archive_entries (zip). Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.