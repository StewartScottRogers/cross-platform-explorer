---
id: CPE-690
title: Virtualize the file list (render only the visible window)
type: feature
component: Frontend
priority: high
status: Open
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
