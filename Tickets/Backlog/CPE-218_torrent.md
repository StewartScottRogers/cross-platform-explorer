---
id: CPE-218
title: Preview/edit support for Torrent metadata files
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed:
---

## Summary

Add a preview provider for Torrent metadata (.torrent) in the right-side preview pane. Files, trackers, info hash. Read-only viewer.

## Acceptance Criteria

- [ ] .torrent is matched by a dedicated preview provider in the bundled registry
- [ ] Viewer: Files, trackers, info hash.
- [ ] Editing: Read-only viewer.
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: bencode parser (JS). Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.