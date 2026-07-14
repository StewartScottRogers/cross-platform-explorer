---
id: CPE-342
title: "Recent folders — an MRU of visited folders on the Home view"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

The Home view tracks recent *files* but not recently-*visited folders*. Add a **Folders** tab
(alongside Recent / Favorites) listing the most-recently-visited folders, click-to-navigate,
with a per-row remove — reusing the tab + row machinery from CPE-338/341.

## Design (frontend-only, reuses existing helpers)
- **settings.ts:** `cpe.recentFolders` key + `loadRecentFolders`/`saveRecentFolders`
  (validated with the existing `isRecentArray`). Mutation reuses the existing `addRecent`
  (MRU, dedupe-by-path, capped at 20) and `removeRecent` helpers — folders share the
  `RecentFile` shape.
- **App.svelte:** in `loadPath`, on a successful non-Home load, record the folder
  (`recordRecentFolder`). The `!error` / `path === HOME` guards already there mean archives
  (list_dir fails) and Home aren't recorded. Load in `applySettings`; pass to HomeView; wire
  `on:removeRecentFolder`. Navigate is already wired.
- **HomeView.svelte:** third pill **Folders**; tab body lists `recentFolders` (folder icon,
  name, path hint), click → `navigate`, ✕ → `removeRecentFolder`; empty state.

## Assumptions (Nightshift — user asleep, logged per policy)
- Back/forward/up all count as "visiting" (any folder you land on enters the MRU) — matches
  how Explorer's recent list behaves and keeps the hook in the single `loadPath` sink.
- Reuse the 20-item cap from `addRecent` rather than introduce a separate limit.

## Acceptance
- Visiting folders populates the Folders tab MRU (newest first, deduped, capped).
- Click navigates; ✕ removes one; survives restart.
- `npm run check` + `npm test` green (incl. a Folders-tab render test).

## Work Log
2026-07-13 — Filed during Nightshift (loop 5). Extends the Home tab pattern once more; picked
because it's genuinely useful, purely frontend, and fully unit/component-testable without a
GUI drive. Implemented on branch `CPE-342-recent-folders`.

Delivered:
- `settings.ts`: `cpe.recentFolders` key + load/save (reuses `isRecentArray`, `addRecent`,
  `removeRecent`).
- `App.svelte`: `recentFolders` state, load in `applySettings`, `recordRecentFolder` called
  from `loadPath` on a successful non-Home load (error/HOME guards keep archives & Home out),
  HomeView wiring incl. `on:removeRecentFolder`.
- `HomeView.svelte`: third **Folders** pill + tab body (navigate on click, ✕ remove, empty
  state), dynamic section heading.
- `HomeView.test.ts`: +2 render tests (Folders tab lists visited folders, navigates, removes).

Verification: `npm run check` 0 errors; 277 tests pass (was 275); `npm run build` ok. Done.
