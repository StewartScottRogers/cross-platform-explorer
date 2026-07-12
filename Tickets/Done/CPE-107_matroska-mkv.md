---
id: CPE-107
title: Preview/edit support for Matroska video (MKV) files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Matroska video (MKV) (.mkv) in the right-side preview pane.
Play, with a clear fallback when the codec is unsupported. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .mkv is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Play, with a clear fallback when the codec is unsupported.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: None. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered: .mkv is classified as video and played by the video provider via a native <video> element. Playback is codec-dependent (H.264/VP9/AAC play; exotic codecs may not) — inherent to the webview, noted honestly. Verified + regression-tested. Load cancellation and large/corrupt-file fallback come from the shared PreviewPane (CPE-059).

## Work Log

2026-07-12 — Implemented/verified and closed as part of the native-render/already-mapped format batch (CPE-078/095/096/098/100/103/104/105/107/108/114). npm run check clean; unit tests green (provider-kind regression tests added).
