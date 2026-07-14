---
id: CPE-118
title: Preview/edit support for 3D models (STL/OBJ/GLTF) files
type: Feature
status: Blocked
priority: Medium
component: Frontend
tags: [resource-blocked, needs-heavy-dep]
estimate: 4h+
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for 3D models (STL/OBJ/GLTF) (.stl/.obj/.gltf) in the right-side preview pane.
Interactive 3D viewer (orbit/zoom). Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .stl/.obj/.gltf is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Interactive 3D viewer (orbit/zoom).
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: three.js (bundle-size review). Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Work Log

2026-07-12 — Triaged during the backlog sweep. Deferred to Blocked/: needs a capability that can't be delivered by a pure-Rust change verifiable in this environment (see Notes). Not declined — parked with an owner checklist.

## Notes

**Blocked on:** 3D model preview (STL/OBJ/GLTF) needs an interactive WebGL viewer (e.g. three.js) — a large frontend dependency whose rendering and GPU behaviour cannot be verified headlessly.

**Unblocks when:** the owner checklist below is done and the result is verified on a real display / with the native toolchain.

### Next Actions — Owner
- [ ] Bundle a self-contained WebGL viewer (three.js) under the CSP; add a "model" preview kind
- [ ] Parse STL/OBJ/GLTF; render with orbit controls
- [ ] Review bundle-size impact against PURPOSE.md; verify on a real GPU/display
