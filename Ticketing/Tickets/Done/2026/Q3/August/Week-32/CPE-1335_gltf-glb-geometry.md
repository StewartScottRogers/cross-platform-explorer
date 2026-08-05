---
id: CPE-1335
title: "3D-model reader: add glTF + GLB geometry/stats (serde_json, no new dep)"
type: feature
component: cpe-server
priority: low
status: Done
tags: ready
created: 2026-08-05
epic: CPE-118
---

## Summary
CPE-1333 built `crates/server/src/model_3d.rs` for STL + OBJ and added a `FileType::Glb` detection arm, but
`read_model_info` does not yet PARSE glTF/GLB. glTF is JSON (`serde_json` is already a dependency — NO new dep)
and GLB is a 12-byte header + a JSON chunk, so mesh/accessor counts fall out cleanly. This extends the reader to
the third common 3D format.

## Build
- Extend `crates/server/src/model_3d.rs` `read_model_info` to handle:
  - **GLB (binary glTF):** magic `b"glTF"` + `u32` version + `u32` total length; then chunks — the first chunk
    (type `JSON`, `0x4E4F534A`) is the glTF JSON. Parse that JSON with `serde_json`.
  - **glTF (text/JSON):** the file IS the glTF JSON.
  - From the JSON, report what's cleanly available: `ModelFormat::Gltf` (add the enum arm), and counts derivable
    from the document — e.g. mesh count, and vertex/triangle counts IF an accessor `count` for `POSITION` /
    indices is readily available (be honest: if per-primitive geometry counts require dereferencing accessors,
    report what's directly available — mesh/node count — rather than fabricating a triangle count). Bounding box:
    glTF accessors carry `min`/`max` for POSITION — use them if present; else leave the bbox zeroed (documented).
  - Keep the `ModelInfo` shape; if triangle/vertex are not reliably derivable for glTF, set them to what's honest
    (e.g. 0 with a comment, or fill from accessor counts if present). Prefer HONEST partial data over guessing.
- **Panic/DoS safety (same bar as CPE-1333):** validate the GLB header/length/chunk bounds before slicing;
  `serde_json::from_slice(...).ok()?` (malformed JSON → `None`, never panic); bound any allocation by the
  already-existing 128MB command read cap. No `unwrap` on untrusted bytes.
- Update `file_type.rs` if glTF (`.gltf` JSON) needs a detection arm beyond the existing `Glb` magic.

## Acceptance criteria
- `read_model_info` returns `ModelFormat::Gltf` with sensible, HONEST fields for: an inline minimal glTF JSON
  document, and an inline GLB (12-byte header + JSON chunk) built in the test. Bounding box from accessor
  min/max when present.
- Malformed GLB (bad magic / truncated / lying length) and malformed JSON return `None`, not a panic (explicit
  tests, same as the STL truncation tests).
- `cargo test -p cpe-server` green; `cargo clippy --all-targets -D warnings` clean in both feature modes; if
  `ModelInfo`/`ModelFormat` changes shape, REGENERATE `bindings.gen.ts` (drift guard). No new dependencies.

## Notes
- BACKEND — full 3-OS backend CI + Typed-bindings drift guard. Adding a `ModelFormat::Gltf` enum arm changes the
  specta type → bindings regen required.
- Independent of CPE-1334 (frontend). Reference: `model_3d.rs` (CPE-1333), the existing `serde_json` usage in
  the crate.
