---
id: CPE-1334
title: "Surface 3D-model geometry in the preview/info pane (wire read_model_info)"
type: feature
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-05
epic: CPE-118
---

## Summary
CPE-1333 shipped the backend `read_model_info` command (`readModelInfo` binding) returning
`ModelInfo { format, triangle_count, vertex_count, bounding_box, ascii }` for STL/OBJ (and GLB detection). It's
currently wired to NO frontend — a registered command with no caller. This ticket surfaces it: when a 3D-model
file (STL/OBJ, +GLB once CPE-1335 lands) is selected, show its geometry stats in the preview/info pane — the
metadata-pane fallback epic CPE-118 documents (the interactive WebGL viewer remains the attended/blocked part).

## Build
- Find where the frontend shows per-file info for other types (study `PreviewPane.svelte` and/or
  `PropertiesDialog.svelte` — how images/media/documents surface their metadata; match that pattern + placement).
- For a selected file the type detector classifies as a 3D model (STL/OBJ/GLB — reuse the existing file-type
  detection; the `FileType` already has `Stl`/`Obj`/`Glb` arms), call `commands.readModelInfo(path)` and render:
  - Format (STL / OBJ / glTF-binary), ASCII-vs-binary for STL, triangle count, vertex count, and the bounding-box
    dimensions (width×height×depth derived from `bounding_box` `[min_xyz, max_xyz]`).
  - Note OBJ's `triangle_count` is a FACE count (per the backend doc) — label it honestly ("faces" for OBJ, or a
    generic "faces/triangles" label) rather than asserting triangles.
- Handle gracefully: a file that returns `null`/None (not actually a parseable model) shows nothing / a neutral
  state, never an error toast. Follow the busy-cursor `invoke` convention (via the generated `commands` client).
- Use a stale-response guard (generation token) if selection can change while the async call is in flight
  (match the pattern other async info-panes use).

## Acceptance criteria
- Selecting an STL or OBJ file shows its format + geometry stats (tri/face count, vertex count, dimensions) in
  the info/preview pane; a non-model file shows no 3D section.
- `npm run check` clean; a jsdom test mocks `readModelInfo` and asserts the 3D section renders the returned
  stats for a model file and is absent for a `null` result + stale-response is dropped. No new deps.
- (If practical) a `gui-smoke` spec seeds a tiny STL/OBJ and asserts the 3D info renders on the real build —
  optional but valued; if added, RUN it (don't just type-check).

## Notes
- FRONTEND-ONLY (backend command already exists) — merge on the Frontend CI job.
- Conflict surface: `PreviewPane.svelte` or `PropertiesDialog.svelte` (whichever is the right home) + a small
  helper/i18n. Independent of CPE-1335 (which is backend `model_3d.rs`).
- Reference: how `PreviewPane.svelte`/`PropertiesDialog.svelte` render existing per-file metadata; `bindings.gen.ts`
  `readModelInfo`/`ModelInfo`/`ModelFormat`.
