---
id: CPE-766
title: Virtualize the icons & gallery grid views (variable tile height + auto-fill columns)
type: feature
status: Open
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-19
closed:
epic: CPE-688
estimate: 3-4h
---

## Summary
Follow-up to CPE-690, which virtualized the **details** view only (clean fixed row height, single
column). The icons and gallery **grid** views still render a DOM node per entry, so a 10k-file folder in
icons/gallery pays the full cost CPE-690 removed for details. This ticket extends windowing to those two
grid views to satisfy the original CPE-690 goal ("only the visible window in all three views").

Grids are harder than the details list: tile height is variable (icons vs. gallery thumbnails) and the
column count is a dynamic `auto-fill` responsive to pane width — so the windowing math needs
rows-of-N-columns rather than one-row-per-entry. The pure windowing helper already takes a `columns`
param (`src/lib/virtualize.ts`, added in CPE-690), so the foundation exists; this is the render
integration + measuring columns-per-row and tile height from the live grid.

## Acceptance Criteria
- [ ] Icons and gallery views render only the visible window (+overscan) of tiles for large folders.
- [ ] Column count and tile height are measured from the live grid (survive pane resize / view switch).
- [ ] Keyboard nav (including up/down across a row), selection, scroll-into-view, rename-in-place, and
      drag/drop all still work with windowed tiles — same guarantees CPE-690 gives details.
- [ ] Small folders and the details view are unaffected; `npm run check` + suite green; GUI-verified.

## Notes
- Prereq/foundation: `windowRange` + `ensureVisibleOffset` in `src/lib/virtualize.ts` (CPE-690) already
  accept a `columns` param and are unit-tested. Reuse the same spacer technique CPE-690 used for details.
- Epic CPE-688 (Explorer performance 10×). Sibling of CPE-692 (slim row template).
- Attended GUI verification required (jsdom has no layout — same as CPE-690).
