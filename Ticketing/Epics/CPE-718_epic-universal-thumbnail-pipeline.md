---
id: CPE-718
title: "EPIC: Universal thumbnail pipeline"
type: Task
status: In Progress
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

## Work Log

- 2026-07-23: Activated. First slice CPE-939 — the pure, std-only thumbnail cache core in `cpe-server`
  (`thumb_cache.rs`): stable collision-resistant cache key (path+mtime+size+target_px), dual-budget
  (count + bytes) LRU cache, and request coalescing. Headless-testable; the per-format extractors +
  frontend streaming client build on top of it.

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** Per-format thumbnail extractors + frontend streaming client unbuilt (only cache core).

## Re-activated 2026-08-01 (workshift) — DoD-gap assessment + decomposition
PM scouting (grep-first; the 2026-07-30 note was accurate this time). TRUE state:
- **Image path complete + cached** (thumbnail.rs, thumb_source.rs, thumb_orient.rs) + the CACHE and
  QUEUE cores are BUILT (thumb_cache.rs CPE-939, thumb_queue.rs CPE-950, cargo-tested) but **ORPHANED**
  — declared modules wired into NO command; `ThumbnailImage.svelte` bypasses both with a naive per-tile
  eager decode. (The CPE-978 orphaned-but-built leverage pattern.)
- **No per-format extractors** beyond raster images: zero SVG / font / video / PDF thumbnailing.

Decomposition (headless-buildable first; heavy-dep formats deferred):
- **CPE-1236** — SVG + font glyph-sheet thumbnail extractors in cpe-server (extend thumb_source dispatch;
  lighter deps resvg/usvg + ab_glyph/fontdue; cargo-tested). Cohesive backend slice (shared dispatch +
  Cargo, so ONE ticket, not two).
- **CPE-1237** — Frontend streaming thumbnail client: wire the built `thumb_queue` (priority
  Visible>Prefetch>Background) + `thumb_cache` into visible virtualized tiles, replacing the naive eager
  decode in `ThumbnailImage.svelte`. vitest + gui-smoke. (Prereq 1236 for shared integration points.)
- **CPE-1238 (deferred)** — Video representative-frame + PDF first-page extractors. HEAVY native deps
  (ffmpeg/pdfium/mupdf) that fight PURPOSE's fast/small/predictable, and real-render verification is
  GUI/hardware-gated. Build-to-last-mile then defer to the user.
