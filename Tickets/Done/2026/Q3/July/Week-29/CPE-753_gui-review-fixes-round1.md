---
id: CPE-753
title: GUI review round 1 — book icon, treemap Up speed, button tooltips, menu no-wrap
type: bug
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-19
closed: 2026-07-19
estimate: 1h
---

## Summary
First round of fixes from the attended GUI verification of the sidecar build (CPE-747/748/751):
1. The **book icon** (Documents toolbar button, CPE-747) looked bad — replace with a clean open-book glyph.
2. The treemap **"Up" button is slow** (CPE-751) — it re-ran a recursive scan of the parent every time.
3. **Buttons should show a hover tooltip** — the treemap Up button was `disabled` at the root, which
   suppresses its `title` tooltip entirely.
4. **Menu items must never wrap** — one line always (regression risk from adding menu icons in CPE-748).

## Fixes
- `Icon.svelte` — new `book` glyph: an open book (two facing pages + centre spine), `currentColor`.
- `DiskSpaceView.svelte` — cache scanned children per path (`cache[dir]`); Up / re-drill of a visited
  folder is now instant (no recursive re-scan). Up button uses `aria-disabled` + an `.off` class instead
  of the `disabled` attribute, so its `title` tooltip always shows; tooltip text clarified.
- `MenuBar.svelte`, `ContextMenu.svelte`, `TabMenu.svelte`, `AgentMenu.svelte` — `white-space: nowrap` on
  every menu item row so labels never wrap; the menu grows to fit.

## Acceptance
- [x] The Documents toolbar icon is a recognizable open book.
- [x] Treemap Up (and re-drilling a visited folder) is instant — served from the per-path scan cache.
- [x] The Up button shows its tooltip on hover even at the root (no longer a `disabled` element).
- [x] No menu item wraps to a second line in any popup menu (MenuBar / context / tab / agent).
- [x] `npm run check` clean; treemap tests still pass.

## Notes
On the CPE-751 branch (treemap under review), so one rebuilt sidecar carries the treemap + these fixes.
