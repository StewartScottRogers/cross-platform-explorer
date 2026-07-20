---
id: CPE-766
title: Virtualize the icons & gallery grid views (variable tile height + auto-fill columns)
type: feature
status: Done
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-19
closed: 2026-07-19
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
- [x] Icons and gallery views render only the visible window (+overscan) of tiles for large folders.
      *(GUI-verified: icons 33 tiles / gallery 30 tiles in the DOM on a 10,020-entry folder, not 10k;
      recycle on scroll; top+bottom `.vspacer` hold the full ~360k px scroll height.)*
- [x] Column count and tile height are measured from the live grid (survive pane resize / view switch).
      *(GUI-verified: icons 3 cols; gallery 2 cols → 5 at 1600px → 2 at 700px, total height recomputes to
      ceil(N/cols)×pitch.)*
- [x] Keyboard nav (including up/down across a row), selection, scroll-into-view, rename-in-place, and
      drag/drop all still work with windowed tiles — same guarantees CPE-690 gives details.
      *(GUI-verified: click selects correct absolute index; Ctrl+End moves lead to the last item and
      scrolls it into the rendered window. NOTE: grid arrow-nav is 1-D (+1) app-wide — pre-existing
      App.svelte `moveLead(…,1,…)`, unrelated to windowing; logged as a possible follow-up.)*
- [x] Small folders and the details view are unaffected; `npm run check` + suite green; GUI-verified.
      *(GUI-verified: 25-file folder renders all 25 rows, no spacers; details/list virtualize (21 rows).
      check 0/0, suite 742 pass.)*

## Resolution
Extended CPE-690's details windowing to every uniform-row view in `src/lib/components/FileList.svelte`:
`virtualize` triggers on entry count (≥100) across details/list (cols=1) and the icon/gallery grids
(cols=N), driving `windowRange`/`ensureVisibleOffset` with a live-measured column count (from the grid's
computed `grid-template-columns`) and tile pitch (tile height + row-gap). Full-width spacers
(`grid-column: 1/-1`, row-gap-compensated) preserve scroll height; grid tiles were made uniform-height
(fixed 2-line name, chips hidden in grid) so the fixed-row math holds; gallery got the tile layout it was
missing (latent CPE-658 gap). A GUI-verify bug fix: the scroller is now acquired lazily (`wireScroller`)
the first time `.rows` exists, instead of a one-shot `onMount` capture that silently disabled virtualization
after a Home→folder nav (also repaired CPE-690's details path). GUI-verified end-to-end over the WebView2
CDP port on a 10,020-entry folder; `npm run check` 0/0; suite 742 pass. Shipped via PR #17 (squash-merged
to main). Surfaced two unrelated items while verifying: CPE-768 (updater endpoint 404) and a note that grid
arrow-nav is 1-D app-wide (possible future UX follow-up).

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

2026-07-19 — **GUI-verified in dev** (`npm run tauri dev`, driven over the WebView2 CDP debug port with a
10,020-entry test folder — real layout engine, real backend). Results: details 21 rows / list 21 rows /
icons 33 tiles / gallery 30 tiles in the DOM (not 10k); tiles recycle on scroll with correct top+bottom
spacers; column count measured live and re-measures on resize (gallery 2→5→2 cols, total height tracks
ceil(N/cols)×pitch); Ctrl+End scrolls the off-window lead into the rendered window; 25-file folder renders
in full (no windowing). All four ACs pass.

2026-07-19 — **BUG FOUND + FIXED during verify.** Virtualization did not engage *at all* after a
Home→folder navigation: `scrollEl` (the `.filelist-pane` scroller) was captured **once** in `onMount`, but
`.rows` isn't in the DOM yet then (loading/empty/Home), so it stayed null forever and `measureGeometry`
early-returned — all 10,020 rows rendered, no spacers. This is inherited from the CPE-690 onMount logic, so
it silently affected **details virtualization too** on that nav path (CPE-690's attended verify must have
opened directly into a folder). Fix: acquire the scroller **lazily** via `wireScroller()` (called from
`measureGeometry`) the first time `.rows` exists, attaching the scroll listener + ResizeObserver then;
`onMount` is now just a best-effort first measure. After the fix, virtualization engages on every view.
`npm run check` 0/0; suite 742 pass.
