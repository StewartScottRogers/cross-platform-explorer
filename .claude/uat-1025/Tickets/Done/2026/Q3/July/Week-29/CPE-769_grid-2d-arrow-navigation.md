---
id: CPE-769
title: 2-D arrow-key navigation in the icons/gallery grid views
type: feature
status: Done
priority: low
component: Frontend
tags: ready
created: 2026-07-19
closed: 2026-07-19
epic: CPE-688
estimate: 1-2h
---

## Summary
In the icons and gallery **grid** views, arrow keys move the selection lead **1-D** (ArrowDown = next
item, ArrowUp = previous item) rather than **2-D** (ArrowDown = down one row = +columns, ArrowUp = up one
row = −columns; ArrowLeft/Right = ∓1). Windows Explorer / Finder move by a full row in grid views, so the
current behaviour feels off once tiles are laid out in a grid. Details/list views (single column) are
correct as-is.

Found while GUI-verifying CPE-766 (grid virtualization): clicking a tile selects the right item and
ArrowDown moved +1 (to the next tile) instead of down a row. This is pre-existing App behaviour, unrelated
to virtualization — `src/App.svelte` handles ArrowDown/ArrowUp with `moveLead(selection, ±1, …)` regardless
of view; there is no per-view grid column count feeding the arrow math. (`gridCols` in App.svelte is the
app *shell* layout template, not the file-grid column count.)

## Acceptance Criteria
- [x] In icons/gallery views, ArrowDown moves the lead down one row (+columns) and ArrowUp up one row
      (−columns); ArrowLeft/ArrowRight move ∓1, wrapping across row boundaries sensibly.
      *(GUI-verified, 3-col icons: n011 →Down n014 (+3) →Right n015 (+1) →Up n012 (−3) →Left n011 (−1).)*
- [x] Details and list views keep their current 1-D up/down behaviour.
      *(GUI-verified: details Down/Up move ±1; Right is a no-op.)*
- [x] Movement clamps at the first/last item (no wrap past the ends); Shift+Arrow extends selection the
      same way. *(GUI-verified clamp: Up from n002 → n001, no underflow. Shift handled by moveLead's
      existing range path — covered by selection tests.)*
- [x] Works with the virtualized window (CPE-766): moving the lead off the rendered window scrolls it into
      view — `moveLead` sets the absolute index; FileList's `ensureLeadVisibleVirtual` (CPE-766, already
      GUI-verified) scrolls off-window leads in. Delta correctness is what CPE-769 adds.

## Notes
- The grid's live column count is already measured in `FileList.svelte` (CPE-766) via the computed
  `grid-template-columns`; App's arrow handler needs that column count (or the view + a shared helper) to
  compute the next index. Consider surfacing the measured `cols` from FileList (event/prop) or computing it
  in App from the same source of truth to avoid drift.
- Epic CPE-688 (explorer performance/polish). Low priority — a navigation nicety, not a correctness bug.

## Work Log
2026-07-19 (nightshift, 23:22 MST) — Picked up. Estimate: 1-2h (kept). Plan: pure `arrowDelta(key, cols)`
helper + unit tests (headless verification), wire into App's key handler measuring live grid columns; keep
the change localized to App.svelte (+ new helper) — no FileList/ExplorerPane plumbing.

2026-07-19 (nightshift, 23:24 MST) — Implemented. `moveLead(sel, delta, count, shift)` already clamps to
[0,count-1], so grid nav only needed the right delta:
- New pure `src/lib/gridnav.ts` `arrowDelta(key, cols)`: Down=+cols, Up=−cols, Right=+1, Left=−1 for
  grids; Down=+1/Up=−1/Left=Right=0 for a single column; 0 for non-arrow keys. Unit-tested
  (`gridnav.test.ts`, 6 cases incl. cols<1/NaN/floor guards).
- `App.svelte`: `currentGridCols()` reads the resolved `grid-template-columns` off the live `.rows.grid`
  (same source of truth FileList windows against, CPE-766) — 1 for list/details. ArrowDown/Up now use
  `arrowDelta`; added ArrowLeft/Right cases that act only when cols>1 (list/details Left/Right unchanged).
- Verified headless: `npm run check` 0/0; full suite **748 pass** (742 + 6 new). No regression.
- GUI verify (CDP, 60-file folder in icons view) pending — confirming Down moves a whole row, Right by one
  tile, and off-window lead scrolls in (CPE-766 windowing interplay).
