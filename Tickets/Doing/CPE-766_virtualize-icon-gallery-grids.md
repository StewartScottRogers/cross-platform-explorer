---
id: CPE-766
title: Virtualize the icons & gallery grid views (variable tile height + auto-fill columns)
type: feature
status: In Progress
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
      *(implemented headlessly; awaiting attended GUI verify — jsdom has no layout)*
- [x] Column count and tile height are measured from the live grid (survive pane resize / view switch).
- [ ] Keyboard nav (including up/down across a row), selection, scroll-into-view, rename-in-place, and
      drag/drop all still work with windowed tiles — same guarantees CPE-690 gives details.
      *(implemented; awaiting attended GUI verify)*
- [x] Small folders and the details view are unaffected; `npm run check` + suite green; GUI-verified.
      *(headless half done: check 0/0, suite 742 pass, <100-entry folders skip windowing; GUI verify pending)*

## Notes
- Prereq/foundation: `windowRange` + `ensureVisibleOffset` in `src/lib/virtualize.ts` (CPE-690) already
  accept a `columns` param and are unit-tested. Reuse the same spacer technique CPE-690 used for details.
- Epic CPE-688 (Explorer performance 10×). Sibling of CPE-692 (slim row template).
- Attended GUI verification required (jsdom has no layout — same as CPE-690).

## Work Log
2026-07-19 — Picked up. Estimate: 3-4h (kept). Plan: generalize the CPE-690 details windowing to every
uniform-row view (details/list columns=1, icons/gallery grids columns=N) using `windowRange`'s existing
`columns` param. Measure column-count (from the live grid's computed `grid-template-columns`) and tile
pitch (tile height + row-gap) so they survive resize/view-switch. Make grid tiles uniform-height so the
fixed-row math holds: fixed 2-line name box, and hide the tag *chips* in grid views (keep the colour dot).
Spacers span the full grid width (`grid-column: 1/-1`), compensated for the grid row-gap.

2026-07-19 — Discovery: gallery view (`.row.view-gallery`) has **no tile layout** — it fell back to the
base 30px details-grid row with an overflowing 128px thumbnail (a latent CPE-658 gap). Virtualizing a
broken layout is meaningless, so extend the icon tile layout (flex-column, centered, 2-line name) to
gallery as part of this work — it both fixes the light-table rendering and makes gallery tiles uniform
for windowing. Logged as in-scope for "virtualize the gallery grid view".

2026-07-19 — Implemented in `FileList.svelte` (headless half). All changes:
- Generalized the CPE-690 details windowing to **every uniform-row view**: `virtualize` now triggers on
  entry count alone (≥100), and `windowRange`/`ensureVisibleOffset` are called with a measured `cols`
  (1 for details/list, N for grids). Rows still carry ABSOLUTE index ⇒ selection/rowEls/DnD/rename
  untouched. Off-window keyboard lead scroll is grid-aware via `cols`.
- `measureGeometry` now reads **columns** from the live grid's computed `grid-template-columns`
  (auto-fill resolves to concrete tracks — count them) and **tile pitch** = tile height + `row-gap`;
  re-measured on scroll (rAF), mount, resize (ResizeObserver, jsdom-guarded), and view/entry change.
- Spacers span the full grid width (`.vspacer { grid-column: 1/-1 }`) and their heights are compensated
  by one `row-gap` in grids (the spacer is itself a grid row) so tiles land at their true positions.
- Grid tiles made **uniform-height** (the fixed-row math's precondition): the name box is a FIXED 2
  lines (`height: 2.5em`, was max-clamp only), and tag **chips are hidden in grid** views (`.rows.grid
  .tag-chips { display:none }`) — the colour dot still flags a tag, full chips remain in details/list.
  Tiles get `overflow:hidden` so a stray Agent-Watch badge can't grow one tile past its row.
- Gallery given the icon tile layout (shared `.row.view-icons, .row.view-gallery` rules), fixing the
  latent no-layout gap.
- Verified headlessly: `npm run check` 0 errors / 0 warnings; full JS suite **742 pass** (incl. the
  `virtualize.ts` columns unit tests). No Rust touched.
- **NOT closed — needs attended GUI verify** (jsdom has no layout): windowing correctness while
  scrolling icons/gallery, column-count re-measure on pane resize, keyboard up/down across a grid row
  landing on the right tile, rename of a scrolled-to tile, DnD, and that details/list/small folders are
  unchanged. Committed to branch `CPE-766-virtualize-grids`.
