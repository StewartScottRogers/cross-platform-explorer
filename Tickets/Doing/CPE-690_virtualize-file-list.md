---
id: CPE-690
title: Virtualize the file list (render only the visible window)
type: feature
component: Frontend
priority: high
status: In Progress
tags: needs-prereq
created: 2026-07-18
epic: CPE-688
estimate: 4h+
---

## Summary
Headline of CPE-688. Render only the visible window (+ overscan) in details/icons/gallery views instead of
a DOM node per entry, so a 10k-file folder paints in a fixed cost. Keep keyboard nav, selection,
scroll-into-view, rename-in-place, and drag/drop working with windowed rows (the `rowEls` ref array and
`scrollIntoView` in App.svelte become window-aware). Hand-rolled windowing, fixed row-height per view.

## Acceptance Criteria
- [ ] Only the visible window + overscan is in the DOM in all three views; large folders paint fast.
- [ ] Keyboard nav, selection, scroll-into-view, rename-in-place, drag/drop all still work.
- [ ] No small-folder regression; `npm run check` + suite green; GUI-verified (scroll/select/rename).

## Notes
Attended: windowed rows vs. selection/DnD/rename/keyboard interactions need live GUI verification.

## Work Log
2026-07-18 (dayshift) — Landed the safe headless foundation: pure windowing math in src/lib/virtualize.ts (windowRange + totalHeight; visible index range + overscan + spacer heights; list & grid via a columns param), 7 unit tests (caught+fixed a past-end-scroll edge bug). The FileList render integration — windowed rows coexisting with keyboard nav / selection / scroll-into-view / rename / DnD — remains the attended, GUI-verified part.

## Work Log (continued)
2026-07-19 — Picked up (attended). Estimate: 4h+ (kept). Plan: virtualize the DETAILS view only this pass
(clean fixed-height, columns=1); icons/gallery grids deferred to a follow-up (variable tile height +
dynamic auto-fill columns need a separate design). User chose "details view first, grids as follow-up".

2026-07-19 — Implemented details-view virtualization in `FileList.svelte`:
- Scroll container is the ancestor `.filelist-pane` (`overflow-y:auto`, holds the sticky `.columns`
  header); found on mount via `rowsEl.closest(".filelist-pane")`.
- Spacer technique: `.rows` keeps its true height via top/bottom `.vspacer` divs, so the ancestor
  scroller + sticky header are unchanged. `windowRange`/`ensureVisibleOffset` (CPE-690 foundation) drive
  the window; `effScroll = containerTop - rowsTop` (measured via getBoundingClientRect) handles the
  header offset without fragile offsetTop math. Row height measured from a live row (falls back to 30px).
- Rows render with their ABSOLUTE index (`windowed = entries.slice(win.start,win.end).map(...i)`), so
  every selection / `rowEls[i]` / DnD / rename path is byte-for-byte unchanged.
- **Gated**: only `view==="details" && entries.length >= 100` virtualizes. Small folders and the
  icons/gallery grids render in FULL exactly as before — the common case and PURPOSE.md "plain explorer
  stays predictable" constraint pay nothing.
- Off-window keyboard lead: App's `rowEls[lead].scrollIntoView` can't reach a non-rendered row, so
  FileList scrolls the container for off-window leads only (`ensureLeadVisibleVirtual`); in-window leads
  and all non-virtualized behaviour keep the existing scrollIntoView. `ResizeObserver` guarded for jsdom.
- Verified headlessly: `npm run check` 0/0; full JS suite **708 pass**. No Rust touched.

2026-07-19 — **NOT closed — needs live GUI verification.** jsdom has no layout, so the ≥100-entry
virtualized path (window correctness, scroll, keyboard nav across the window edge, rename of a
scrolled-to row, DnD, range-select spanning the fold) is unverified. Committed to branch
`CPE-690-virtualize-details`; **not merged to main** (core-explorer change must not ship unverified).
Remaining to close: (1) attended GUI verify of the above in details view; (2) file/great the follow-up
for icon+gallery grid virtualization (AC "all three views").
