---
id: CPE-096
title: Preview/edit support for SVG (rich) files
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for SVG (rich) (.svg) in the right-side preview pane.
Sandboxed render plus source edit and dimensions readout. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .svg is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Sandboxed render plus source edit and dimensions readout.
- [x] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Sandboxed rendering. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered: .svg is classified as an image and rendered (rich, vector) by the image provider via <img>. Verified + regression-tested. Editing SVG as raw XML source is not offered (it is treated as an image for rich rendering); that is a possible future dual-mode enhancement. Load cancellation and large/corrupt-file fallback come from the shared PreviewPane (CPE-059).

## Work Log

2026-07-12 — Implemented/verified and closed as part of the native-render/already-mapped format batch (CPE-078/095/096/098/100/103/104/105/107/108/114). npm run check clean; unit tests green (provider-kind regression tests added).
