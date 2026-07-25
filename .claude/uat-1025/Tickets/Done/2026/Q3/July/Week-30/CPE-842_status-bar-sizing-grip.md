---
id: CPE-842
title: Add a window sizing grip to the lower-right of the status bar
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
The main window's status bar had no explicit resize affordance in its lower-right corner. Added the
classic **sizing grip** there — a diagonal-hatch handle that starts an OS-window resize from the
bottom-right corner when dragged.

## Acceptance Criteria
- [x] A sizing grip renders in the lower-right corner of the status bar, always in the corner regardless
      of status-bar content.
- [x] Dragging it resizes the window from the bottom-right (Tauri `startResizeDragging("SouthEast")`).
- [x] Theme-variable coloured (identical light/dark), `nwse-resize` cursor; harmless no-op outside Tauri
      (test harness).
- [x] `npm run check` clean (0 errors, 0 warnings); StatusBar tests pass.

## Resolution
- `src/lib/components/StatusBar.svelte` — import `getCurrentWindow` from `@tauri-apps/api/window`; a
  `.resize-grip` `<div>` (last child of `.statusbar`) with an `on:mousedown` that calls
  `getCurrentWindow().startResizeDragging("SouthEast")`, guarded in try/catch so it's a no-op outside
  Tauri. Styled as three diagonal strokes via a `repeating-linear-gradient` clipped to the lower-right
  triangle (`clip-path`), coloured with `var(--text-faint)`, `cursor: nwse-resize`, brightening on hover.
  A `svelte-ignore a11y-no-noninteractive-element-interactions` documents the deliberate mouse-only
  affordance.
- `src/app.css` — `.statusbar { position: relative }` so the grip anchors absolutely in the corner.

Verification: `npm run check` → 0 errors / 0 warnings; `vitest StatusBar.test.ts` → 8 passed. The visible
grip + drag-to-resize ride the next install of a build carrying this change.

## Work Log
- 2026-07-21 — Added the corner grip using Tauri's `startResizeDragging` (already used `getCurrentWindow`
  elsewhere). Confirmed the exact v2 API/`ResizeDirection` union from the installed types. check clean,
  StatusBar tests green. Closing.
