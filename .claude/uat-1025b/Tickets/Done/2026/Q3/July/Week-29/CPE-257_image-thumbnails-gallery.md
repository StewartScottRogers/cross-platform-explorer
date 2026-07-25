---
id: CPE-257
title: Image thumbnails in the Icons view (gallery)
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
estimate: 1-2d
created: 2026-07-13
---

## Summary

The Icons view currently shows generic type icons. Turn it into a real gallery by
rendering downscaled image thumbnails for image files, so a photos folder is
browsable visually. This is the highest-value remaining explorer feature and
lines up with the disabled "Gallery" nav item in the Sidebar.

Deferred from Nightshift because it is GUI-heavy to verify and needs interactive
testing on real photo folders — best done with the user present.

## Design notes / open questions

- **Where to decode.** The `image` crate is currently built with only `png`+`tiff`
  decoders (Cargo.toml), so a backend `read_thumbnail` would need jpeg/gif/webp/bmp
  features added — that grows the binary (weigh against the fast/small/predictable
  tiebreaker). Alternative: downscale in the frontend via a canvas (browser decodes
  jpeg/png/gif/webp for free), avoiding new backend codecs.
- **Performance.** Must stay fast/small/predictable with the mode off. Thumbnails
  should be lazy (only for visible rows, e.g. IntersectionObserver), cached, and
  bounded in size. Large folders must not load full-size images eagerly.
- **Caching.** Consider an in-memory LRU keyed by path+mtime; optionally an on-disk
  cache later.

## Acceptance Criteria

- [ ] Image files in the Icons view show a downscaled thumbnail instead of a
      generic icon; non-images keep their icon.
- [ ] Thumbnails load lazily for on-screen rows only and are cached.
- [ ] No startup or large-folder regression when not in Icons view.
- [ ] Tests cover the thumbnail generation/resize path.

## Work Log
2026-07-13 — Filed during Nightshift as the specified home for the deferred
gallery/thumbnail work. Relates to the Icons ViewMode and Sidebar "Gallery" stub.

2026-07-13 (Dayshift) — Implemented the **frontend canvas** approach from the design
notes (no new Rust image codecs → binary stays small; the WebView decodes
jpeg/png/gif/webp/bmp/svg for free). Branch `CPE-257-image-thumbnails`.

- `src/lib/thumbnails.ts`: `canThumbnail`, `fitDimensions` (aspect-preserving, never
  upscales), `thumbKey` (path+mtime so edits bust the cache), a bounded `ThumbCache`
  (LRU, cap 240), and `makeThumbnail` (Image→canvas→data-URL, shares in-flight decodes).
- `FileList.svelte`: in Icons view, image files get a `.thumb-slot` that lazy-loads its
  thumbnail via IntersectionObserver (rootMargin 150px; eager fallback where IO is
  absent, e.g. jsdom). Falls back to the generic icon on any decode failure — a broken
  image never blanks a tile. Details/List views and non-images are untouched.
- `App.svelte`: passes `assetUrl={convertFileSrc}` so the WebView can load files.
- Tests: `thumbnails.test.ts` (extension gate, fit math incl. no-upscale/zero guards,
  cache key, LRU eviction) + FileList render tests (image gets exactly one thumb-slot;
  txt/folder keep icons; details view has none).

Assumptions (Dayshift, user away): lazy per-tile load + LRU(240) + 96px box satisfies the
"fast/small/predictable when off" constraint (thumbnails only build in Icons view, only
for on-screen image tiles). Reused the existing asset-protocol capability (PreviewPane
already uses convertFileSrc for images), so no new capability needed.

**VISUAL QA PENDING (honest):** `npm run check` (0 errors), full vitest (843 passing incl.
the new suites), and `npm run build` all pass — but the actual pixel rendering of
thumbnails can't be observed from this headless harness (native WebView2 window; jsdom
has no real canvas decode). The wiring is verified at DOM level; the on-screen result
should be eyeballed on a real photo folder. Filed CPE-343-style caveat here rather than
claiming visual confirmation.
