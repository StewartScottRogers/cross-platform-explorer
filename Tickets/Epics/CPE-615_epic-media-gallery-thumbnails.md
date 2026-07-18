---
id: CPE-615
title: "EPIC: Media gallery & thumbnails"
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

Make the explorer **visual for media folders**: real image/video **thumbnails** in the icon view, a
dedicated **gallery mode** (responsive grid with large previews), a spacebar **quick-look** overlay,
and richer media metadata (EXIF/dimensions/duration) in the details pane. Today images show a generic
icon and the preview pane is one-at-a-time.

## Why

A huge share of "browsing files" is really browsing photos and videos. Generic icons make a photo
folder useless; thumbnails + a gallery grid turn it into a usable light-table. This is an **additive
mode** — the plain list/details views stay untouched — so it respects the fast/small/predictable
tiebreaker while adding high-visible value for the media case.

## Rough scope (areas, not child tickets)

- A backend thumbnail service: decode + downscale images (and a video first-frame) to a bounded cache,
  generated lazily and off the UI thread; strict size/time caps like the search engines.
- A persistent, size-capped thumbnail cache keyed by path+mtime; evict on staleness.
- Thumbnails wired into the existing icon view; a new **gallery mode** (bigger tiles, minimal chrome).
- Quick-look: spacebar opens a large, keyboard-navigable preview overlay (←/→ through the folder).
- Media metadata in the details pane / properties: dimensions, EXIF (camera, date-taken, GPS?),
  video duration/codec — reusing the properties infrastructure.

## Open questions (resolve at activation)

- Which formats in v1? (JPEG/PNG/GIF/WebP baseline; HEIC/RAW/video are heavier — some are already
  Blocked tickets on decode libraries: [[CPE-097]] HEIC, [[CPE-102]] RAW.)
- Thumbnail decode: pure-Rust crates vs OS thumbnail APIs (Windows `IThumbnailProvider`)? Cross-platform
  cost vs quality.
- Cache location/size policy and how it interacts with the sidecar catalog cache conventions.
- Does gallery mode become a first-class "view" alongside details/list/icons, or a separate mode?

## Definition of Done

- Image folders show real thumbnails in icon/gallery view, generated lazily without janking scroll.
- Gallery mode + spacebar quick-look navigate a folder's media smoothly.
- Details/properties show dimensions + basic EXIF for supported images.
- The thumbnail cache is bounded, stale-aware, and never blocks the plain views.
- With media features unused, the plain explorer's startup/memory are unchanged (verified).
