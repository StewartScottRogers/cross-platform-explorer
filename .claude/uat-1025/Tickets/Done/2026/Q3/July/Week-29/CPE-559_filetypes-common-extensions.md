---
id: CPE-559
title: "Filetypes — recognize more common extensions (Photoshop, e-books, disc images, extra A/V)"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Extend the filetype tables (CPE-048 precedent) so more everyday files get the right icon category + a
human type name instead of falling through to "unknown". Missing common ones: `psd`; e-books `epub`/`mobi`;
disc/disk images `iso`/`dmg` + `cab`; extra audio `wma`/`aiff`/`aif`/`mid`/`midi`; extra video
`mpg`/`mpeg`/`3gp`/`mts`/`m2ts`; Linux `appimage`.

## Acceptance Criteria
- [x] `CATEGORY_BY_EXT` maps the above to sensible categories (image / document / archive / audio / video /
      executable).
- [x] `TYPE_NAME_BY_EXT` gives each a readable name (e.g. "Photoshop image", "EPUB e-book", "Disc image").
- [x] `npm run check` clean; `filetypes.test.ts` covers a representative sample of the new mappings.

## Resolution
Extended both tables with ~20 common extensions: `psd`→image; `epub`/`mobi`/`pages`→document;
`iso`/`dmg`/`cab`/`lz`/`lzma`→archive; `wma`/`aiff`/`aif`/`mid`/`midi`→audio;
`mpg`/`mpeg`/`3gp`/`mts`/`m2ts`→video; `appimage`→executable — each with a readable `TYPE_NAME` (e.g.
"Photoshop image", "EPUB e-book", "Disc image", "AppImage application"). Purely additive; no existing
mapping changed, no new i18n. `filetypes.test.ts` +2 cases (12 category + 4 type-name assertions), 29
passed; `npm run check` 0/0.

## Notes
Purely additive — no behaviour change for already-recognized extensions, no new i18n strings (categories
already have their labels). Consistent with the existing table conventions.
