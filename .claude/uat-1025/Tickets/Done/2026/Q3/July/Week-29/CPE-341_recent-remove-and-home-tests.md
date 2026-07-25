---
id: CPE-341
title: "Remove a single Recent entry + HomeView component tests"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

Two small, related improvements to the Home view:

1. **Remove one recent** — today the Recent list only has "Clear" (all). Add a hover ✕ on
   each recent row to drop just that entry, mirroring the remove-star on Favorites (CPE-338).
2. **HomeView component tests** — CPE-338/340 shipped with logic + type coverage but no
   component render test (I couldn't drive the native WebView2 window during Nightshift).
   Add render tests that assert the Favorites tab lists starred items and the Recent remove/
   clear controls fire — compensating cover for the deferred GUI drive.

## Design (frontend-only)
- **settings.ts:** pure `removeRecent(list, path)` helper (+ unit test).
- **HomeView.svelte:** per-row ✕ on recent rows → `removeRecent` dispatch; reuse the same
  hover affordance styling as the favorites star.
- **App.svelte:** wire `on:removeRecent` to filter `recents` and persist.
- **HomeView.test.ts (new):** render with pins/recents/favorites; assert Favorites-tab items
  render, a folder favorite dispatches `navigate` and a file favorite `openFile`, and the
  recent ✕ dispatches `removeRecent`.

## Acceptance
- Hovering a recent row shows a ✕ that removes just that entry and persists.
- New component tests pass; whole suite green; `npm run check` clean.

## Work Log
2026-07-13 — Filed during Nightshift (loop 4). CPE-332 (default models) was considered but
is blocked: it needs the external reference's per-agent model IDs, which are not in this
repo, and guessing IDs would risk breaking agent launches. Chose this safe, testable UX win
instead, which also hardens the Favorites work with the component test it lacked.
Implemented on branch `CPE-341-recent-remove-and-home-tests`.

Delivered:
- `settings.ts`: pure `removeRecent(list, path)` (+ 2 unit tests).
- `HomeView.svelte`: hover ✕ on each recent row → `removeRecent`; grid widened to a third
  column; matching hover styling.
- `App.svelte`: `on:removeRecent` filters + persists recents.
- `HomeView.test.ts` (new, 3 tests): renders the Favorites tab and asserts folder→navigate /
  file→openFile dispatch + empty state, and that the recent ✕ dispatches the right path.
  This render-level test is the real verification of CPE-338's Favorites UI that the native
  window couldn't give us.

Verification: `npm run check` 0 errors; 275 tests pass (was 270). Done.
