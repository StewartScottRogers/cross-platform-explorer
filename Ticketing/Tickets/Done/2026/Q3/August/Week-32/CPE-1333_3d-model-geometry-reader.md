---
id: CPE-1333
title: "3D-model geometry reader (STL + OBJ) — cpe-server engine + file-type signatures + command"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-05
epic: CPE-118
---

## Summary
Epic CPE-118 (3D model support) is Blocked ONLY on the interactive WebGL/three.js viewer (GPU + attended). But
the ticket explicitly says the format *"falls back to the metadata pane for edit intent"* — and that fallback
was never built. This ticket builds the pure-Rust, cargo-testable geometry/stats reader that IS that fallback:
a "3D model" metadata surface (triangle/vertex counts, bounding box, format), the exact analog of the shipped
media-metadata work (`media_column.rs`/`doc_column.rs`). **Zero new dependencies** — STL/OBJ are hand-rollable.

## Build
- **New `crates/server/src/model_3d.rs`:** `pub fn read_model_info(bytes: &[u8]) -> Option<ModelInfo>` (or a
  `Result`) returning `ModelInfo { format, triangle_count, vertex_count, bounding_box: [f32;6], ascii: bool }`
  (match the crate's existing column/info struct conventions — read `media_column.rs` / `binary_preview.rs`):
  - **Binary STL:** 80-byte header + `u32` triangle count + 50 bytes/triangle. Derive vertex bbox from the
    triangle records.
  - **ASCII STL:** `solid` / `facet normal` / `outer loop` / `vertex` line parsing; count facets + vertices, bbox
    from vertex lines.
  - **OBJ:** line-based text — count `v` (vertices) and `f` (faces), bbox from `v` coordinates.
  - **Binary-vs-ASCII STL disambiguation:** an ASCII STL starts with the word `solid`, but some BINARY STLs also
    embed `solid` in the 80-byte header — do NOT rely on the prefix. Validate the binary invariant
    `80 + 4 + 50*n == file_len`; if it holds treat as binary, else parse as ASCII. Add this as an explicit test.
- **`crates/server/src/file_type.rs`:** add `FileType` arm(s) + magic/extension detection for 3D models (STL,
  OBJ; and the trivial glTF `b"glTF"` GLB magic if cheap) so the type detector recognises them (the enum
  currently has no 3D variants). Keep parity with the existing detector's test style.
- **Thin command:** add a `#[tauri::command]` dispatcher in `src-tauri/src/lib.rs` (one-liner into cpe-server,
  async + `spawn_blocking` per the async-command convention), register it in `generate_handler!`, and REGENERATE
  the specta bindings (`bindings.gen.ts`) — editing a `specta::Type` struct requires the bindings regen or CI's
  Typed-bindings drift guard fails. (Frontend consumption — a column/pane — is a follow-up, not this ticket.)

## Acceptance criteria
- `read_model_info` correctly returns triangle/vertex counts + bbox + ascii flag for: a small inline BINARY-STL
  fixture (e.g. a 2-triangle, ~134-byte byte array), an inline ASCII-STL, and an inline ASCII OBJ cube — all as
  inline `&[u8]` fixtures (no external files), matching the crate's inline-fixture convention.
- The binary-vs-ASCII disambiguation is covered by a test (incl. a binary STL whose header contains "solid").
- `file_type.rs` detects STL/OBJ (+ glTF if included); tests cover the new signatures.
- `cargo test -p cpe-server` green; `cargo clippy --all-targets -D warnings` clean in BOTH feature modes;
  `bindings.gen.ts` regenerated (drift guard green). No new dependencies (challenge any you think you need).

## Notes
- BACKEND — needs the full 3-OS backend CI + Typed-bindings drift guard (not just Frontend). Do NOT assert exact
  filesystem byte counts in tests (cross-OS); assert geometry counts + bbox from the parsed data.
- Reference: `crates/server/src/media_column.rs` / `doc_column.rs` (column/info pattern), `file_type.rs`
  (detector + test style), `binary_preview.rs` (inline-fixture convention).
- Follow-up (separate ticket): wire `read_model_info` into a 3D-model metadata column/pane in the frontend.
