---
id: CPE-615
title: "EPIC: Media gallery & thumbnails"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed: 2026-07-18
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

## Decisions (2026-07-18, activated in dayshift — best-guess)
- **v1 formats:** whatever the `image` crate decodes (JPEG/PNG/GIF/WebP/BMP/TIFF). HEIC/RAW/video stay
  Blocked (CPE-097/102) — no change here.
- **Thumbnail generation:** backend `thumbnail(path, max_edge)` → PNG data URL via `image::thumbnail`
  (fast aspect-preserving downscale). Generated lazily by the frontend, off the main thread.
- **Cache:** a size-capped on-disk cache keyed by path+mtime is a follow-up child (v1 returns fresh).
- **Gallery mode / quick-look:** follow-up children once thumbnails render in the icon view.

## Child tickets (just-in-time)
1. CPE-642 — Backend `thumbnail(path, max_edge)` PNG data URL (cargo-tested).
2. CPE-643 — Frontend: lazy thumbnail loading in the icon view (image files show real thumbnails).
3. CPE-644 — On-disk thumbnail cache (path+mtime keyed, size-capped).
4. CPE-645 — Gallery mode (bigger tiles) + spacebar quick-look.

## Resolution (closed 2026-07-18)
All six children delivered the media gallery:
- CPE-642 — backend `thumbnail(path, max_edge)` PNG data URL.
- CPE-643 — lazy thumbnails in the icon view.
- CPE-644 — on-disk, size-capped, mtime-keyed thumbnail cache.
- CPE-645 — spacebar quick-look overlay (←/→ navigation).
- CPE-648 — post-thumbnail cleanup.
- CPE-658 — gallery view mode (large photo tiles).
- CPE-659 — image dimensions + EXIF in Properties (also enabled JPEG/GIF/WebP/BMP
  decoding, which fixed real-photo thumbnails).

DoD gates met: real thumbnails in icon/gallery view (lazy, non-janking); gallery mode +
quick-look; dimensions + basic EXIF in Properties; bounded stale-aware cache; plain views
untouched (additive mode). No carve-outs. GPS EXIF was left out of v1 (marked optional "?"
in the brief) — a candidate follow-up if wanted.
