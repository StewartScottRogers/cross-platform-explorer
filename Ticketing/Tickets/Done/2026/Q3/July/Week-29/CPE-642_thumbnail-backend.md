---
id: CPE-642
title: Backend thumbnail command (PNG data URL)
type: feature
component: Backend
priority: medium
status: Done
tags: ready
estimate: 1h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-615
---

## Summary
First child of CPE-615 (media gallery). `thumbnail(path, max_edge)` decodes an image and returns a
downscaled PNG `data:` URL (longest edge ≤ max_edge, aspect preserved) for grid tiles. Bounded by the
preview size cap; errors on non-images so the frontend can fall back to a generic icon.

## Acceptance Criteria
- [x] Pure `make_thumbnail_png(path, max_edge)` via `image::thumbnail`; cargo-tested (downscale +
      aspect + non-image error).
- [x] `thumbnail` command wraps it as a data URL, size-capped; registered in `generate_handler!`.
- [x] cargo test + clippy clean.

## Work Log
2026-07-18 (dayshift) — Built the thumbnail backend. Lazy loading in the icon view (CPE-643) + on-disk
cache (CPE-644) follow.
