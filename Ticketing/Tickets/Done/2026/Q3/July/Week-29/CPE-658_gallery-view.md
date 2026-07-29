---
id: CPE-658
title: Gallery view mode (large photo tiles)
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-615
---

## Summary
Child of CPE-615 (media gallery). A fourth view mode, **Gallery** — the icon grid with larger tiles
and bigger thumbnails (128px), a photo light-table. Reachable from the View dropdown and the command
palette; pairs with the spacebar quick-look (CPE-645). Completes the media-epic UI.

## Acceptance Criteria
- [x] `ViewMode` gains `"gallery"`; FileList renders it as a wider grid (`minmax(184px)`) with 128px
      thumbnails / 88px fallback icons.
- [x] View dropdown + palette command ("View: Gallery"); persists via `saveView`.
- [x] `view.gallery` + `palette.viewGallery` added to all 12 locales (coverage gate green).
- [x] `npm run check` clean; suite green.

## Work Log
2026-07-18 (nightshift) — Added gallery mode on top of the thumbnail infra; media epic UI complete.
