---
id: CPE-1336
title: "3D info pane: render glTF/GLB (format label + mesh count; don't show a bare '0 triangles')"
type: bug
component: frontend
priority: high
status: Done
tags: ready
created: 2026-08-05
epic: CPE-118
---

## Summary
CPE-1334 built the 3D info pane in `PreviewPane.svelte` when `ModelFormat` was only `Stl`/`Obj`. CPE-1335 (now
merged) added a `Gltf` variant + a `mesh_count` field. The info pane does NOT handle `Gltf`, so a glTF/GLB file
now renders a **blank Format row** (the format-label ternary falls through to `""`) and defaults the count label
to **"Triangles"** showing **0** — which the CPE-1335 UAT explicitly warned is misleading (glTF triangle/vertex
counts are honestly 0 because they aren't derivable without accessor dereferencing). Neither the TS compiler nor
the existing tests catch this (both fallbacks are valid strings). Fix the rendering so glTF shows honest,
useful info.

## Build
In `src/lib/components/PreviewPane.svelte`'s `.model-info` section:
- **Format label:** extend the `modelFormatLabel` ternary with a `Gltf` arm → "glTF" (covers both `.gltf` and
  `.glb`; the existing `ascii` flag already distinguishes text-glTF vs binary-GLB if you want an "(binary)" hint,
  optional).
- **For glTF, show `mesh_count`** (a new "Meshes: N" row) INSTEAD of the triangle/face count — and **suppress the
  triangle/face and vertex rows when they're 0 for glTF** (don't print "0 Triangles"/"0 Vertices", which is
  misleading). For STL/OBJ, keep the existing triangle/face + vertex rows unchanged (mesh_count is 0 for those —
  don't show a Meshes row there).
- Keep the bounding-box **dimensions** row for glTF when the bbox is non-zero (glTF carries POSITION min/max);
  if the bbox is all-zero (no accessor extrema), suppress the dimensions row rather than showing "0 × 0 × 0".
- Add an i18n key for "Meshes" (and any glTF label string) to ALL 12 COMPLETE_LOCALES.

## Acceptance criteria
- Selecting a glTF/GLB file that parses shows Format "glTF", a "Meshes" count, and dimensions (when available) —
  and does NOT show a bare "0 Triangles"/"0 Vertices" row.
- STL/OBJ rendering is unchanged (triangles/faces + vertices still shown; no Meshes row).
- A glТF with no accessor extrema shows no "0 × 0 × 0" dimensions row.
- `npm run check` clean; the `PreviewPane.model.test.ts` suite is extended: a glTF `ModelInfo` (format `Gltf`,
  mesh_count N, triangle/vertex 0, non-zero bbox) renders Format "glTF" + "Meshes N" + dimensions and NOT
  "Triangles"/"0 Vertices"; and a Gltf with zero bbox omits the dimensions row. i18n 12 locales. No new deps.

## Notes
- FRONTEND-ONLY (backend + bindings already have `Gltf`/`mesh_count`) — merge on the Frontend CI job.
- Completes the 3D-model feature (CPE-118 metadata-pane fallback: STL/OBJ/glTF/GLB). Source: reviewer + UAT
  findings on #630/#631.
- Reference: `PreviewPane.svelte` `.model-info` section (`modelFormatLabel`/`modelCountLabel`/`modelDims`),
  `bindings.gen.ts` `ModelInfo`/`ModelFormat` (now with `Gltf` + `mesh_count`).
