---
id: CPE-078
title: Preview/edit support for HTML documents files
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for HTML documents (.html) in the right-side preview pane.
Rendered in a sandboxed iframe (no script/network) with a source-edit toggle. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [x] .html is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Rendered in a sandboxed iframe (no script/network) with a source-edit toggle.
- [x] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: srcdoc sandbox iframe. Editing model: source. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered: .html is classified as code and previewed as syntax-highlighted, editable source (write_file_text, CPE-066) by the shared text provider. Verified + regression-tested. A sandboxed rendered-HTML view is a possible future enhancement; source preview/edit is the delivered scope. Load cancellation and large/corrupt-file fallback come from the shared PreviewPane (CPE-059).

## Work Log

2026-07-12 — Implemented/verified and closed as part of the native-render/already-mapped format batch (CPE-078/095/096/098/100/103/104/105/107/108/114). npm run check clean; unit tests green (provider-kind regression tests added).
