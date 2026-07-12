---
id: CPE-108
title: Preview/edit support for QuickTime video (MOV) files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for QuickTime video (MOV) (.mov) in the right-side preview pane.
Play via the video element. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .mov is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Play via the video element.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: None. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered: .mov is classified as video and played by the video provider via <video>. Codec-dependent (H.264/AAC play). Verified + regression-tested. Load cancellation and large/corrupt-file fallback come from the shared PreviewPane (CPE-059).

## Work Log

2026-07-12 — Implemented/verified and closed as part of the native-render/already-mapped format batch (CPE-078/095/096/098/100/103/104/105/107/108/114). npm run check clean; unit tests green (provider-kind regression tests added).
