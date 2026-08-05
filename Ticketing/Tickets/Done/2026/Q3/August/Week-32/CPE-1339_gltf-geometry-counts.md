---
id: CPE-1339
title: "glTF/GLB: real vertex + triangle counts from accessor.count (no buffer deref)"
type: feature
component: cpe-server
priority: low
status: Done
tags: ready
created: 2026-08-05
epic: CPE-118
---

## Summary
`model_3d.rs` currently reports `triangle_count = 0` / `vertex_count = 0` for glTF/GLB, with a doc comment saying
real counts "need per-primitive accessor dereferencing". **That's actually not needed for the COUNTS** — a glTF
accessor carries a `count` field in the JSON itself, so vertex/triangle counts are derivable from the JSON alone
(no reading of buffer/BIN bytes). Fill them in honestly.

## Build
In `read_model_info`'s glTF/GLB path (the JSON already parsed for mesh_count + bbox):
- **vertex_count** = sum over all mesh primitives of `accessors[primitive.attributes.POSITION].count` (guard:
  missing attributes/POSITION/accessor/count → skip that primitive, don't panic).
- **triangle_count** = sum over primitives, honoring `primitive.mode` (glTF default 4 = TRIANGLES):
  - indexed (has `indices`): let `n = accessors[indices].count`. mode 4 (TRIANGLES) → `n/3`;
    mode 5 (TRIANGLE_STRIP) or 6 (TRIANGLE_FAN) → `n.saturating_sub(2)`; modes 0/1/2/3 (points/lines) → 0.
  - non-indexed: use the POSITION accessor `count` as `n` in the same per-mode formula.
  - unspecified mode → treat as 4 (the glTF default).
  - Sum with `saturating_add` (no overflow panic). Keep it HONEST — if a primitive lacks the data to count, skip it.
- Update the module + `ModelInfo` field doc comments (they currently say counts are always 0 for glTF) to reflect
  that counts now come from accessor `count` fields (bbox still from accessor min/max).
- **Panic/DoS safety (same bar as CPE-1333/1335):** all accessor/index lookups via `.get(i)?` / `.ok()?`;
  `saturating_*` arithmetic; a malformed accessor index / missing field → skip, never panic. No allocation driven
  by a declared count.

## Acceptance criteria
- An inline glTF with a POSITION accessor (`count` = 24) and an indices accessor (`count` = 36, mode default) →
  `vertex_count == 24`, `triangle_count == 12`. A TRIANGLE_STRIP primitive (indices count = 10, mode 5) → 8
  triangles. A non-indexed TRIANGLES primitive (POSITION count = 9) → 3 triangles + 9 vertices. A points/lines
  primitive → 0 triangles (vertices still counted).
- Multi-primitive / multi-mesh sums are correct. Malformed (bad accessor index, missing count) → those skipped,
  no panic (explicit test). GLB path derives the same from its JSON chunk.
- `cargo test -p cpe-server` green; clippy clean both modes. `ModelInfo` shape UNCHANGED (only fills existing
  fields) → NO bindings regen needed (confirm bindings.gen.ts is untouched). No new deps.

## Notes
- BACKEND — 3-OS CI (no bindings change → no drift). Serializes on `model_3d.rs` (nothing else touches it now).
- Frontend already renders `triangle_count`/`vertex_count`; note that CPE-1336 made glTF show a "Meshes" row
  INSTEAD of tri/vertex. Once real counts land, a tiny follow-up could show tri/vertex for glTF too — out of
  scope here (this ticket only makes the backend numbers honest). Reference: the glTF JSON parsing already in
  `model_3d.rs` (mesh_count + bbox from accessors).
