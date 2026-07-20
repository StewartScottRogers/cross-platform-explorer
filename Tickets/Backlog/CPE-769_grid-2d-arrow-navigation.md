---
id: CPE-769
title: 2-D arrow-key navigation in the icons/gallery grid views
type: feature
status: Open
priority: low
component: Frontend
tags: ready
created: 2026-07-19
closed:
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
- [ ] In icons/gallery views, ArrowDown moves the lead down one row (+columns) and ArrowUp up one row
      (−columns); ArrowLeft/ArrowRight move ∓1, wrapping across row boundaries sensibly.
- [ ] Details and list views keep their current 1-D up/down behaviour.
- [ ] Movement clamps at the first/last item (no wrap past the ends); Shift+Arrow extends selection the
      same way.
- [ ] Works with the virtualized window (CPE-766): moving the lead off the rendered window scrolls it into
      view (the `ensureVisibleOffset`/`ensureLeadVisibleVirtual` path already handles this once the target
      index is correct).

## Notes
- The grid's live column count is already measured in `FileList.svelte` (CPE-766) via the computed
  `grid-template-columns`; App's arrow handler needs that column count (or the view + a shared helper) to
  compute the next index. Consider surfacing the measured `cols` from FileList (event/prop) or computing it
  in App from the same source of truth to avoid drift.
- Epic CPE-688 (explorer performance/polish). Low priority — a navigation nicety, not a correctness bug.
