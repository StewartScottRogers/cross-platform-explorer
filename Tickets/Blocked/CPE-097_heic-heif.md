---
id: CPE-097
title: Preview/edit support for HEIC / HEIF images files
type: Feature
status: Blocked
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for HEIC / HEIF images (.heic/.heif) in the right-side preview pane.
Decode to a viewable image (webviews cannot show HEIC natively). Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .heic/.heif is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Decode to a viewable image (webviews cannot show HEIC natively).
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Backend support: Backend decode (libheif) — lands green via CI (Rust builds/tests locally now too)
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend decode (libheif). Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Work Log

2026-07-12 — Triaged during the backlog sweep. Deferred to Blocked/: needs a capability that can't be delivered by a pure-Rust change verifiable in this environment (see Notes). Not declined — parked with an owner checklist.

## Notes

**Blocked on:** HEIC/HEIF is not decodable by the webview <img> tag and needs a native decoder (libheif). Pure-Rust HEIC decoding is not production-ready, and the result can only be judged on a real display.

**Unblocks when:** the owner checklist below is done and the result is verified on a real display / with the native toolchain.

### Next Actions — Owner
- [ ] Add a HEIC->PNG transcode path (libheif-rs, or platform APIs: macOS ImageIO, Windows WIC) behind a backend command
- [ ] Confirm licensing of the chosen decoder for redistribution
- [ ] Return a data URL / temp PNG the image provider can show; verify visually on each OS
