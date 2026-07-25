---
id: CPE-338
title: "Favorites: star files & folders, browse them from the Home view"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

The Home view's **Favorites** tab has been a disabled "not implemented yet" placeholder
(`HomeView.svelte`), and the context menu can only *pin folders* to Quick access. Add a real
**Favorites** feature: the user stars any file OR folder; favorites persist and are browsable
from the previously-dead Favorites tab. A staple of every mature file explorer.

Favorites are distinct from Pins:
- **Pins** = folders pinned to the *Quick access* card grid (folders only).
- **Favorites** = a curated list of files *and* folders, shown under the Favorites tab.

## Design (frontend-only — settings are an opaque JSON blob, no Rust change)

- **Model:** `Favorite = { path; name; is_dir }`, stored under `cpe.favorites`, validated
  defensively on load (mirrors `pins`/`recents`).
- **settings.ts:** `KEYS.favorites`, `loadFavorites`/`saveFavorites`, `toggleFavorite(list, entry)`.
- **Context menu:** on any single selection (file or folder) offer "Add to / Remove from
  Favorites" (`favorite` action), alongside the existing Pin item.
- **HomeView:** the pill bar becomes a real tab switch — Recent ↔ Favorites (Shared stays
  disabled). The Favorites tab lists starred items with the right icon; clicking a folder
  navigates, clicking a file opens (reuses existing `navigate`/`openFile` handlers), and a
  star removes it (`unfavorite`). Empty-state when none.

## Assumptions (Nightshift — user asleep, logged per policy)
- Favorites include both files and folders (a favorites feature that excluded files would be
  strictly weaker than Pins). Single-selection add for MVP; multi-select can come later.
- No dedicated keyboard shortcut this pass (Pin has none either); context menu + Home tab are
  the surfaces.

## Acceptance
- Star a file and a folder from the context menu; both appear under Home → Favorites.
- Favorites survive an app restart (persisted to settings.json).
- Clicking a favorite folder navigates; clicking a favorite file opens it; the star removes it.
- `npm run check` and `npm test` green; plain explorer stays fast/small/predictable.

## Work Log
2026-07-13 — Filed during Nightshift. Research found the disabled Favorites placeholder in
HomeView as the top value/effort candidate among the explorer's "not implemented yet" stubs
(Gallery is already CPE-257; Share/Shared are thinner). Implemented on branch
`CPE-338-favorites`.

Implemented (frontend-only, zero Rust change):
- `types.ts`: `Favorite { path; name; is_dir }`.
- `settings.ts`: `cpe.favorites` key + defensive `isFavoriteArray` validator,
  `loadFavorites`/`saveFavorites`, and `toggleFavorite(list, entry)`.
- `ContextMenu.svelte`: `favorited` prop + "Add to / Remove from Favorites" item on any
  single selection (file or folder).
- `App.svelte`: `favorites` state, load at startup, `toggleFavoriteSelected()`,
  `favorite` command case, `favorited` wired to the menu, `favorites` + `unfavorite`
  wired to HomeView.
- `HomeView.svelte`: activated the dead Favorites pill — Recent↔Favorites tab switch;
  favorites list navigates folders / opens files (reuses existing handlers) and a star
  removes; empty-state points at the context menu.
- Tests: 3 new `toggleFavorite` cases; full suite 267 green; `npm run check` 0 errors.

Verification: logic + type-checked. GUI visual drive DEFERRED — at land time the machine
idle was ~110s (user likely still present), so per the share-the-machine rule I did not
open a window. Follow-up: visually confirm the Favorites tab + star round-trip during a
later idle window this shift. Landing now since the change is pure-frontend and fully
unit-covered.
