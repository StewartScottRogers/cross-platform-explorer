---
id: CPE-069
title: Make the side panels mouse-resizable with safe minimum widths
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The layout columns are fixed (sidebar 220px, right pane 300px). Let the user drag to resize the
**left sidebar** and the **right details/preview pane** with the mouse, clamped to safe minimum (and
maximum) widths so no panel can be collapsed to nothing or grow to crowd out the file list. Persist the
chosen widths.

## Acceptance Criteria

- [ ] A draggable divider on the sidebar's right edge and on the right pane's left edge (col-resize cursor)
- [ ] Dragging resizes that panel live; the middle file list absorbs the change
- [ ] Safe **minimum** widths enforced (sidebar and right pane each have a floor); sensible maximums too
- [ ] Widths persist across launches (localStorage via `settings.ts`)
- [ ] Text selection is suppressed while dragging
- [ ] Pure clamp helper unit-tested; jsdom test drives a drag and asserts the width changes + is saved
- [ ] `npm run check` clean; suite green; `vite build` clean

## Resolution

Added draggable `.resizer` dividers on the sidebar's right edge and the right pane's left edge (grid
gains 6px columns; `App` sets `.main`'s `grid-template-columns` inline from `sidebarWidth`/`rightWidth`).
Mouse drag adjusts the width live via window mousemove/up listeners, clamped with `clampWidth` to
sidebar [160,480] and right pane [220,560] so no panel collapses or crowds out the list; text selection
is suppressed while dragging (`.main.resizing`). Widths persist via `settings.saveSidebarWidth`/
`saveRightWidth` and are re-clamped on load. `clampWidth` unit-tested; two App jsdom tests drive a drag
(width changes + persists) and assert min-clamp. `npm run check` clean; suite 192 passed; `vite build`
clean.

## Work Log

2026-07-11 — Requested: "adjust the panels with the mouse, all panels adjustable with safe minimum widths." Left sidebar + right pane are the resizable columns; the 1fr content column absorbs the delta.

## Notes

Minimums chosen: sidebar 160px, right pane 220px. Maximums cap each so the content list keeps room.
