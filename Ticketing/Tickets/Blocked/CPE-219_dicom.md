---
id: CPE-219
title: Preview/edit support for DICOM medical images files
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

Add a preview provider for DICOM medical images (.dcm) in the right-side preview pane. Image plus key tags. Read-only viewer.

## Acceptance Criteria

- [ ] .dcm is matched by a dedicated preview provider in the bundled registry
- [ ] Viewer: Image plus key tags.
- [ ] Editing: Read-only viewer.
- [ ] Backend support: backend DICOM decode — verified locally (cargo) and green via CI.
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: backend DICOM decode. Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Work Log

2026-07-12 — Triaged during the backlog sweep. Deferred to Blocked/: needs a capability that can't be delivered by a pure-Rust change verifiable in this environment (see Notes). Not declined — parked with an owner checklist.

## Notes

**Blocked on:** DICOM needs a medical-imaging stack (dicom-rs) with modality/windowing handling; pixel output can only be validated visually and against clinical sample data.

**Unblocks when:** the owner checklist below is done and the result is verified on a real display / with the native toolchain.

### Next Actions — Owner
- [ ] Use dicom-rs to read metadata (patient/study/series) and pixel data
- [ ] Apply window/level; transcode a frame to PNG for the image provider
- [ ] Verify with sample DICOM datasets on a display
