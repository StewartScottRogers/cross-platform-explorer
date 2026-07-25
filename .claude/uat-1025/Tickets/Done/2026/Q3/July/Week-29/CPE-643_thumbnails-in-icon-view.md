---
id: CPE-643
title: Thumbnails in the icon view (lazy)
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
estimate: 1h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-615
---

## Summary
Second child of CPE-615 (media gallery). Image files in the **icons** view now show a real downscaled
thumbnail instead of a generic icon, fetched from the backend `thumbnail(path, max_edge)` command
(CPE-642). Thumbnails load **lazily** — only when a tile nears the viewport — so a folder of hundreds of
photos never fires hundreds of decodes at once. Additive: non-image entries and the list/details views
are visually unchanged.

This supersedes the earlier client-side canvas thumbnailing (CPE-257) in the icon view with the backend
decode path the epic settled on, consolidating on one mechanism.

## Acceptance Criteria
- [x] Self-contained `src/lib/components/ThumbnailImage.svelte` — props `path`, `size`, `fallback`;
      renders the generic `Icon` while loading and on any error; `object-fit: cover`, rounded corners,
      fixed size, theme variables.
- [x] Lazy: fetches via `IntersectionObserver` (150px rootMargin), with an eager fallback where the
      observer is unavailable (jsdom).
- [x] Renders its own image, so it uses `rawInvoke` (untracked) per the BUSY-CURSOR convention — imported
      from the wrapper `src/lib/invoke.ts`, so the invoke boundary guard still passes without an
      allowlist entry.
- [x] Wired into the icons view for image extensions (`jpg jpeg png gif webp bmp tif tiff avif`) via a
      pure `isImage(name)` helper in `filetypes.ts`; unit-tested.
- [x] List/details views unchanged; non-image entries keep their generic icon.
- [x] `npm run check` clean; full `vitest` suite green (658 tests).

## Resolution
- Added `ThumbnailImage.svelte`: an `IntersectionObserver`-gated tile that calls
  `rawInvoke("thumbnail", { path, maxEdge })` and shows an `<img>` on success, else the fallback `Icon`.
- Added `isImage(name)` + `THUMBNAIL_IMAGE_EXTS` to `filetypes.ts` (pure; last-extension, case-insensitive;
  rejects extensionless/dotfile/trailing-dot names) with a dedicated test in `filetypes.test.ts`.
- `FileList.svelte`: replaced the inline CPE-257 canvas-thumbnail machinery (`loadThumb`/`lazyThumb`/
  `thumbs`/`.thumb-slot`) and its `thumbnails.ts` imports with `<ThumbnailImage>` for image entries; the
  now-unused `assetUrl` prop was dropped (and its binding removed from `App.svelte`).
- Updated the two CPE-257 FileList render tests to the new `.thumb` structure; refreshed the explorer
  docs page.

## Work Log
2026-07-18 (dayshift) — Built the lazy icon-view thumbnail. Frontend only; backend `thumbnail` (CPE-642)
untouched. On-disk cache is CPE-644; gallery mode / quick-look is CPE-645.
