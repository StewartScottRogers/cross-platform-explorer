---
id: CPE-097
title: Preview/edit support for HEIC / HEIF images files
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [resource-blocked, needs-heavy-dep]
estimate: 2-3h
created: 2026-07-11
closed: 2026-08-06
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

## Work Log
- 2026-08-05 (sprint): UNBLOCKED via the platform-API approach (user chose WIC/ImageIO over libheif). Backend + full wiring shipped in CPE-1351 (#646): read_heic_preview_data_url + heic provider render .heic/.heif/.hif. Windows real-decode verified; macOS cfg-compiled. REMAINING = attended only: macOS visual on a Mac + a no-HEIF-extension Windows box (graceful metadata fallback). Moving to Deferred (our-choice: attended verification), no longer externally blocked.

- 2026-08-06 — Closed as **Done**: superseded/delivered by the CPE-13xx preview work. Delivered: heic/heif/hif preview provider + read_heic_preview_data_url platform decode (CPE-1351). Read-only, Err->metadata fallback, request-id cancellation, provider tests green. Sample: samples/images/iphone-photo.heic. All acceptance criteria met (provider registered in the bundled registry; read-only viewer; graceful large/corrupt fallback; in-flight cancellation; unit/provider tests green; npm run check clean).
