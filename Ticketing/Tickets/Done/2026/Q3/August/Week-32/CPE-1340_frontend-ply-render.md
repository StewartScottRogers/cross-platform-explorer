---
id: CPE-1340
title: "3D info pane: render PLY (add to MODEL_EXTS, 'PLY' label, 'Faces' count)"
type: bug
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-05
epic: CPE-118
---

## Summary
CPE-1337 added the PLY backend (`ModelFormat::Ply`, now in `bindings.gen.ts`), but the preview-pane info section
(`PreviewPane.svelte`) doesn't handle PLY: `MODEL_EXTS` lacks `"ply"` (so a `.ply` file never triggers the info
call), `modelFormatLabel` has no `Ply` arm (→ blank format), and `modelCountLabel` would default to "Triangles"
(PLY faces are polygons, not guaranteed triangles — should be "Faces" like OBJ). Wire PLY into the UI.

## Build
In `src/lib/components/PreviewPane.svelte`'s `.model-info` section:
- Add `"ply"` to `MODEL_EXTS` (line ~55) so a selected `.ply` file calls `readModelInfo`.
- Add a `Ply` arm to `modelFormatLabel` → `"PLY"`.
- `modelCountLabel`: use "Faces" (`pv.model.faces`) for **both** `Obj` AND `Ply` (PLY reports a face count, not
  guaranteed triangles — same honesty caveat as OBJ); keep "Triangles" for STL.
- PLY has populated `vertex_count` + face count and `mesh_count = 0`, so it correctly renders via the existing
  STL/OBJ (`{:else}`) branch (faces + vertices rows). Binary PLY reports a zeroed bbox — the CPE-1336 general
  "suppress dimensions when bbox all-zero" already handles that (no "0 × 0 × 0"); confirm it applies to PLY.
- No i18n additions needed (reuses `pv.model.faces`/`vertices`/`format`); if a "PLY" label string is desired,
  it's a literal like "STL"/"OBJ" (those aren't translated), so no locale change.

## Acceptance criteria
- Selecting an ASCII `.ply` file shows Format "PLY", a "Faces" count, vertex count, and dimensions; a binary
  `.ply` shows Format "PLY" + Faces + Vertices and NO "0 × 0 × 0" dimensions row (zeroed bbox suppressed).
- STL ("Triangles") / OBJ ("Faces") / glTF ("Meshes") rendering unchanged.
- `npm run check` clean; `PreviewPane.model.test.ts` extended: a PLY `ModelInfo` (format `Ply`, vertex/face
  counts, non-zero bbox) renders "PLY" + "Faces" + dimensions and NOT "Triangles"; a zero-bbox PLY omits the
  dimensions row. Green. No new deps.

## Notes
- FRONTEND-ONLY (backend + `Ply` binding already exist) — merge on the Frontend CI job. Independent of the
  backend `model_3d.rs` lane (CPE-1339). Reference: how CPE-1336 added the `Gltf` arm + Meshes row.
