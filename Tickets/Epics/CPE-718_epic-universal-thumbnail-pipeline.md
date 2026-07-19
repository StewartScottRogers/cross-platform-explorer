---
id: CPE-718
title: "EPIC: Universal thumbnail pipeline"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Extend thumbnailing beyond in-frontend image rasterization into a backend-cached pipeline that renders
previews for videos (representative frame), PDFs (first page), SVG, fonts (glyph sheet), and office/archive
formats — streamed into the icons and gallery views.

## Why
Today only a handful of raster image types get real thumbnails. A general, cached pipeline makes the icons
and gallery views genuinely useful for mixed folders and is the visual backbone of a modern explorer.

## Rough scope (areas, not child tickets)
- A Rust thumbnail service with a worker pool and per-format extractors.
- A central on-disk cache keyed by path + mtime + size, with eviction/size budget.
- A frontend cache client that streams thumbnails into virtualized icon/gallery rows.
- Graceful fallback to type icons when a format can't be rendered.

## Open questions (resolve at activation)
- Format coverage vs. dependency weight (video/PDF rendering crates) and build size.
- Cache location, size budget, and eviction policy.
- Coordination with virtualization (CPE-690) — only request thumbnails for visible tiles.

## Definition of Done
- Video/PDF/SVG/font/office thumbnails render in icons and gallery views, cached across sessions.
- Thumbnails are generated off the UI thread and streamed in; scrolling stays smooth.
- With the feature disabled, views fall back to type icons and incur no thumbnail cost.
