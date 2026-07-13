---
id: CPE-257
title: Image thumbnails in the Icons view (gallery)
type: Feature
status: Open
priority: Medium
component: Backend + Frontend
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
