---
id: CPE-747
title: Toolbar Documents button — use a book icon (not the info glyph)
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-19
closed: 2026-07-19
estimate: 15m
---

## Summary
The toolbar button that opens the built-in Documents dialog to the current section's page
(`NavToolbar` → `on:help` → `openDocs(currentSection())`, CPE-596) currently uses the generic **`info`
(ⓘ)** glyph, which reads as "info/help" rather than "the manual". Swap it for a clear **book** icon so
users recognize it as Documents. Behaviour is unchanged — it still opens `DocsView` to the section's doc
via the `sectionDocs` registry, and stays bound to F1 / the command palette.

## Scope
- Add a monochrome `book` glyph to `Icon.svelte` (inherits `currentColor` like the other toolbar icons —
  NOT the existing colored `ebook` file-type glyph, which would clash with the toolbar).
- Point the `NavToolbar` docs button at `book`; update its `title`/`aria-label` to name Documents.

## Acceptance
- [x] The toolbar Documents button shows a book icon that matches the toolbar's monochrome line style.
- [x] Tooltip/aria clearly says it opens Documents (still notes F1); open-to-section behaviour unchanged.
- [x] `npm run check` clean.

## Notes
No section added, so the docs self-maintaining guard (CPE-579) doesn't apply. Visual placement is fine as-is
in the toolbar row; broader "icons on menu items too" is tracked separately (CPE-748).

## Resolution
Swapped the toolbar Documents button from the generic `info` glyph to a clear book icon; behaviour
unchanged (still opens `DocsView` to the current section's page via the `sectionDocs` registry, F1/palette).

**Files changed:**
- `src/lib/components/Icon.svelte` — new monochrome `book` glyph (Feather-style, inherits `currentColor`
  like the other toolbar icons; deliberately NOT the colored `ebook` file-type glyph).
- `src/lib/components/NavToolbar.svelte` — docs button now `Icon name="book"`; `title`/`aria-label` →
  "Documents for this section (F1)".

**Verification:** `npm run check` → 0 errors/0 warnings. (Icon is a static monochrome SVG matching the
existing toolbar icons; a quick visual glance when the app next runs is enough.)
