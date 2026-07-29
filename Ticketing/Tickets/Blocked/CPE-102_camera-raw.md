---
id: CPE-102
title: Preview/edit support for Camera RAW (CR2/NEF/ARW) files
type: Feature
status: Blocked
priority: Low
component: Multiple
tags: [resource-blocked, needs-heavy-dep]
estimate: 4h+
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for Camera RAW (CR2/NEF/ARW) (.cr2/.nef/.arw) in the right-side preview pane.
Extract and show the embedded JPEG preview. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .cr2/.nef/.arw is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Extract and show the embedded JPEG preview.
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Backend support: Backend RAW preview extract — lands green via CI (Rust builds/tests locally now too)
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend RAW preview extract. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Work Log

2026-07-12 — Triaged during the backlog sweep. Deferred to Blocked/: needs a capability that can't be delivered by a pure-Rust change verifiable in this environment (see Notes). Not declined — parked with an owner checklist.

## Notes

**Blocked on:** Camera RAW (CR2/NEF/ARW) needs a heavyweight RAW demosaicing pipeline (libraw / rawloader) that is format- and camera-specific and only verifiable against real RAW files on a display.

**Unblocks when:** the owner checklist below is done and the result is verified on a real display / with the native toolchain.

### Next Actions — Owner
- [ ] Integrate rawloader (pure Rust) or libraw; render the embedded JPEG preview first as a fast path
- [ ] Backend command returns a viewable image; verify against sample CR2/NEF/ARW files
- [ ] Decide scope: embedded-preview only vs. full demosaic
