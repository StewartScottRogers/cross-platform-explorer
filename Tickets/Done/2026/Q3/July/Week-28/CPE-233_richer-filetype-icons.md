---
id: CPE-233
title: Richer per-type file icons (distinct glyphs for common formats + fallback)
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Today the file list shows one icon per broad *category* (image, code, archive,
document, …) plus an `unknown` fallback. Add distinct, recognizable glyphs for
common specific formats that currently share a generic icon, so the list is
easier to scan — while still giving a clear generic icon to types the app can't
preview.

Icons stay self-contained inline SVGs at 16px (pictorial, not lettered — text
labels are illegible at list size), following the existing page+glyph style.

New glyphs: font, disk image, database, ebook, certificate/key, 3D model, web
page. Mapping is by extension and is kept separate from `categoryOf` so the
preview-provider selection is unaffected.

## Acceptance Criteria

- [ ] New distinct icons render for fonts, disk images, databases, ebooks,
      certificates/keys, 3D models, and web pages.
- [ ] Unknown/undisplayable types still get the generic `unknown` icon.
- [ ] `categoryOf` (preview selection) is unchanged; only the row icon is richer.
- [ ] Icons read cleanly at 16px (details/list) and 40px (icons view).
- [ ] `npm run check` passes; verified visually on the running build.

## Resolution

Added `ICON_BY_EXT` + `iconFor()` in `filetypes.ts` (separate from `categoryOf`
so preview selection is untouched) and new page+glyph SVGs in `Icon.svelte`:
font, disk, database, ebook, certificate, cube (3D), web. All four file-icon
consumers now use `iconFor` (FileList rows, DetailsPane hero, Home recents,
Properties). Pictorial glyphs (not lettered) so they read at 16px; unknown types
still fall back to the generic icon.

Verified: `npm run check` 0/0; 231 frontend tests pass. Ships in 0.10.0 (live
visual check bundled there).

## Work Log

2026-07-12 — Added 7 per-type glyphs + ICON_BY_EXT/iconFor; wired all file-icon consumers. check clean.
2026-07-12 — 231 FE tests pass. Ships in 0.10.0.

## Notes

Adds `ICON_BY_EXT` + `iconFor()` in `filetypes.ts`; `FileList` uses `iconFor`.
New SVG branches in `Icon.svelte`. Chosen because 16px list icons rule out
lettered "PDF/ZIP" badges (illegible small). Relates to the preview registry
(CPE-059/060).
