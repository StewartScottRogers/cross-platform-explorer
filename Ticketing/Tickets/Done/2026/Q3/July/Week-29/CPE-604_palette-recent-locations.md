---
id: CPE-604
title: Command palette lists recent locations
type: feature
component: navigation
priority: low
status: Done
tags: ready
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
epic:
sprint:
---

## Summary

Extend the new command palette (CPE-602) so it also offers the tab's recently-visited folders as
one-keystroke "jump" entries. Reuses the per-tab navigation history — no new UI surface.

## Acceptance Criteria

- [x] A pure `recentPaths(history, max)` returns the distinct prior folders, most-recent-first,
      excluding the current one, capped. Unit-tested.
- [x] The palette (Ctrl+Shift+P) shows a "Recent" group; each entry's label is the folder name and
      its full path is a searchable keyword; choosing it navigates there.
- [x] `npm run check` clean; frontend vitest suite green.

## Resolution
- `src/lib/history.ts` — added pure `recentPaths(h, max=8)` (dedup, current-excluded,
  most-recent-first). Three tests in `history.test.ts`.
- `src/App.svelte` — spread recent-folder commands into `paletteCommands` (group "Recent", label =
  base name, keyword = full path, `run: () => navigate(p)`).

## Work Log
2026-07-17 (Nightshift Loop 3) — Built + verified. Chose to extend the palette rather than add a
Back-button dropdown (which would need a new MENUS.md-compliant menu component) — same value, no new
UI surface, reuses Loop 1.
