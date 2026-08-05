---
id: CPE-1337
title: "3D reader: PLY format (ASCII + binary) — the 4th model format"
type: feature
component: cpe-server
priority: low
status: Done
tags: ready
created: 2026-08-05
epic: CPE-118
---

## Summary
The `model_3d.rs` reader covers STL, OBJ, glTF, GLB (CPE-1333/1335). **PLY** (Polygon File Format /
Stanford triangle format) is the other common mesh format and is not supported. Its header is text and
**declares element counts directly** (`element vertex N`, `element face M`), so vertex/face counts come straight
from the header — no buffer dereferencing needed. Zero new deps.

## Build
- Add `ModelFormat::Ply` and parse in `read_model_info`:
  - **Header (both ASCII + binary PLY):** starts with `ply\n` (or `ply\r\n`); a `format` line
    (`ascii 1.0` / `binary_little_endian 1.0` / `binary_big_endian 1.0`); `element vertex N` and `element face M`
    lines give `vertex_count` and `triangle_count` (label it a FACE count in the doc, like OBJ — PLY faces can be
    polygons); header ends at `end_header`. Set `ascii` = true for `ascii`, false for the binary variants.
  - **Bounding box:** for **ASCII** PLY, parse the vertex rows after `end_header` (the first 3 numbers per row are
    x/y/z per the `property float x/y/z` order — read the vertex property order from the header) to compute the
    bbox. For **binary** PLY, computing the bbox requires decoding the binary vertex block by property layout —
    scope that as optional; if not implemented, leave the bbox zeroed (documented, like glTF-without-extrema).
    Counts still come from the header for binary PLY.
- **`file_type.rs`:** add a `FileType::Ply` arm + detection (magic prefix `ply` + newline; `.ply` extension).
- **Panic/DoS safety (same bar as CPE-1333/1335):** bounds-check header parsing; never `unwrap` on untrusted
  bytes; parse counts with `.parse().ok()?`; a malformed/truncated header → `None`. Don't trust a declared count
  for allocation (counts are just reported numbers here, not used to size a read).

## Acceptance criteria
- `read_model_info` returns `ModelFormat::Ply` with correct `vertex_count` + face count for: an inline ASCII PLY
  (small cube: 8 verts, 6 faces) with bbox from the vertex rows, and an inline binary-little-endian PLY header
  (counts from header; bbox zeroed if binary vertex decode not implemented). `ascii` set correctly.
- Malformed/truncated PLY headers → `None`, not panic (explicit tests).
- `file_type.rs` detects `.ply`.
- `cargo test -p cpe-server` green; `cargo clippy --all-targets -D warnings` clean both feature modes;
  `ModelFormat::Ply` changes the specta type → REGENERATE `bindings.gen.ts` (drift guard). No new deps.

## Notes
- BACKEND — 3-OS CI + drift guard. Serializes on `model_3d.rs` with CPE-1338 (glTF geometry). Frontend
  follow-up (a `Ply` arm in `PreviewPane.svelte`'s `modelFormatLabel`, like glTF got in CPE-1336) is a separate
  small ticket once this lands.
- Reference: the STL/OBJ header/text parsing + inline-fixture tests already in `model_3d.rs`.
