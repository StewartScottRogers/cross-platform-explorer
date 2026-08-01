---
id: CPE-1216
title: "Spotlight overlay component (sectioned, highlighted results) + item feed + frecency"
type: feature
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-08-01
epic: CPE-704
---

## Summary
Part of CPE-704 — the frontend spotlight (folds in CPE-1216 overlay + CPE-1217 item feed + CPE-1218 frecency
into ONE worker since all touch App.svelte). Backed by the CPE-1214 commands.

## Build
- **Overlay** `src/lib/components/Spotlight.svelte` modeled on `CommandPalette.svelte` (theme vars, ↑/↓/Enter/Esc,
  visible border): renders **sectioned** results (Action→Folder→File→Recent) with **matched-position
  highlighting** (from `SpotResult.positions`). Opened by the `spotlight:open` event AND an in-app trigger (so
  it's verifiable without the OS hotkey). First slice renders in the main window.
- **Item feed** `src/lib/spotlightSources.ts` (pure, jsdom-testable): recents (`history.recentPaths`), drives
  (`listDrives`), favorites, action labels (`paletteCommands`), file/folder hits (`find_files_by_name`,
  streamed per [[prefer-streaming-liveness]]) → `sources: [ResultKind, string[]][]` → `spotlight_search`.
- **Frecency** `src/lib/spotlightFrecency.ts` store (`{path,count,last_used_s}`, settings.ts pattern):
  increment on open/reveal; empty query → `spotlight_frecent` default view; activation ordering.
- Activate: open/reveal file, run action. `invoke`/commands via the busy-tracked path.

## Acceptance Criteria
- [ ] jsdom tests for spotlightSources (kind tagging, caps) + frecency store (increment/decay). gui-smoke
      `spotlight.smoke.ts`: open, type, assert ranked+highlighted sectioned rows, Enter activates; default view
      shows most-frecent first. `npm run check` + `npm test` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-704). Consolidates 1216/1217/1218 (shared App.svelte).
  Depends on CPE-1214.
