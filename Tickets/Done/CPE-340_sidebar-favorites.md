---
id: CPE-340
title: "Favorites section in the left Sidebar (always-visible quick access)"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

CPE-338 added Favorites, but they're only reachable via the Home tab. The point of a favorite
is one-click access from anywhere. Surface the user's favorites as a collapsible **Favorites**
section pinned to the top of the left navigation pane (above Home/places), like a browser's
bookmarks. Folders navigate; files open — reusing the handlers Home already dispatches.

## Design (frontend-only)
- **Sidebar.svelte:** new `favorites: Favorite[]` prop and an `openFile` dispatch. Render a
  collapsible "Favorites" section (only when non-empty) above the Home item; each row is
  icon + name; click a folder → `navigate`, a file → `openFile`. A folder favorite that is
  the current folder highlights via the existing `isMarked` logic.
- **App.svelte:** pass `{favorites}`; wire `on:openFile` to `openRecent` (same path Home
  uses); the existing `on:navigate` already handles folders.
- Removal stays where it already is (Home tab star + context menu) — the sidebar is
  quick-access only, keeping the surface uncluttered.

## Assumptions (Nightshift — user asleep, logged per policy)
- Section only shows when there is ≥1 favorite (no empty header noise).
- Collapsed/expanded state is transient (not persisted) this pass — matches the Home
  section twisties, which are also transient.

## Acceptance
- Starring an item (from anywhere) makes it appear in the Sidebar Favorites section.
- Clicking a favorite folder navigates; a favorite file opens.
- Section hidden when there are no favorites.
- `npm run check` + `npm test` green.

## Work Log
2026-07-13 — Filed during Nightshift (loop 3). Continues the Favorites thread (CPE-338):
research confirmed the app is otherwise mature (sort, view, breadcrumbs, status bar all
present), so the highest-value cohesive gain is making the just-added favorites reachable
persistently. Implemented on branch `CPE-340-sidebar-favorites`.

Implemented (frontend-only):
- `Sidebar.svelte`: `favorites` prop + `openFile` dispatch; collapsible "Favorites" section
  at the top of the nav pane (shown only when non-empty); folder rows navigate, file rows
  open; current-folder favorite highlights via existing `isMarked`.
- `App.svelte`: passes `{favorites}`, wires `on:openFile` to `openRecent`.

Verification: `npm run check` 0 errors; 270 tests pass; `npm run build` ok. No new unit
tests — the change is markup/wiring over already-tested helpers (`toggleFavorite`, `iconFor`).
GUI drive not performed (native WebView2 window; user active) — same rationale as CPE-338/339;
recommend a human eyeball. Done.
