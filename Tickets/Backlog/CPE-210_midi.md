---
id: CPE-210
title: Preview/edit support for MIDI music files
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a preview provider for MIDI music (.mid/.midi) in the right-side preview pane. Track and note-event view. Read-only viewer.

## Acceptance Criteria

- [ ] .mid/.midi is matched by a dedicated preview provider in the bundled registry
- [ ] Viewer: Track and note-event view.
- [ ] Editing: Read-only viewer.
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: MIDI parser (JS). Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.