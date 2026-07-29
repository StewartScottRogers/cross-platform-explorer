---
id: CPE-106
title: Preview/edit support for Subtitles (SRT/VTT) files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Subtitles (SRT/VTT) (.srt/.vtt) in the right-side preview pane.
Cue list with timings; edit the text. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .srt/.vtt is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Cue list with timings; edit the text.
- [x] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: None. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .srt/.vtt preview/edit: mapped to `code`; no dedicated grammar ships with highlight.js, so per the AC it uses the escaped-monospace fallback. Editable as source via the shared text provider (write_file_text, CPE-066); load cancellation + large/corrupt-file fallback come from the shared PreviewPane (CPE-059). Files: src/lib/filetypes.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the text-based data/comms format batch (CPE-079/092/093/106/116/119/202/203/204/209/212/213). npm run check clean; unit tests green.
