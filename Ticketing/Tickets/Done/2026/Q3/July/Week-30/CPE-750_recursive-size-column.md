---
id: CPE-750
title: "Size (recursive)" sortable folder-size column in details view
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-19
closed: 2026-07-20
epic: CPE-706
estimate: 3-4h
---

## Summary
Child of CPE-706. Add an opt-in **"Size (recursive)"** details-view column that shows each folder's
computed subtree size (files already show their own size). Fills on demand via the existing `dir_size`
command — lazily, only for visible rows — cached per path, and sortable. Answers "which folder here is
big?" inline, before opening the treemap.

## Scope
- A pickable column (reuse the columns model / CPE-707 if it lands first, else a dedicated toggle) that,
  when on, requests `dir_size` for each **visible** folder row and renders the result (spinner/"…" until
  it resolves).
- Cache results per path (invalidate on refresh / folder change); never block the initial listing paint
  (compute after first paint, coordinate with virtualization CPE-690 so only on-screen rows compute).
- Sort by the recursive size (folders by computed size, files by own size); stable when some are pending.

## Acceptance
- [x] Turning the column on fills folder sizes lazily for visible rows without stalling the listing.
- [x] Values cache and the column sorts correctly (pending rows sort last / stable).
- [x] No cost when the column is off; no regression to open/scroll speed (CPE-688).

## Notes
Uses the existing `dir_size`. Independent of CPE-749 (which powers the treemap). Headless-testable for the
cache/sort logic; the on-demand fill wants a GUI glance.

## Work Log
- 2026-07-20 — Picked up. Estimate 3-4h. Prereqs present: `dir_size` command, the columns model
  (`columns.ts`), and `sortEntries`. CPE-707 (pickable columns) hasn't landed, so per the ticket's Scope
  ("else a dedicated toggle") I implement this as an **opt-in toggle that fills the existing Size column**
  for folders (which today render blank there) — reusing the whole 4-column model, no variable-width column
  plumbing, and answering "which folder is big?" inline exactly as specified.
- 2026-07-20 — Backend untouched: reuses the existing `dir_size`. Frontend:
  - `sort.ts` — `sortEntries`/`compareEntries` take an optional `sizeOf` so the size key can compare folders
    by recursive subtree size; a pending folder resolves to `-1` and falls back to name order (stable while
    filling).
  - `settings.ts` — `cpe.showFolderSizes` (bool, default **false**).
  - `FileList.svelte` — new `showFolderSizes` + `folderSizes` props; the Size cell shows a folder's cached
    size (or "…" pending) only when on, else blank as before; a reactive dispatches `needSizes` for the
    **visible** (virtualized `windowed`) folders not yet cached.
  - `ExplorerPane.svelte` — forwards the two props + the `needSizes` event.
  - `App.svelte` — `folderSizes` Map + `pendingSizes` dedup; `fillFolderSizes` calls `dir_size` via
    `rawInvoke` (no busy cursor) and reassigns the Map to react; the cache invalidates on every listing/
    refresh; `sizeOf` feeds the sort when on; a Settings-menu checkbox + a palette command toggle it
    (i18n keys `cmd.folderSizes` / `palette.{show,hide}FolderSizes` added to all 12 locales).
- 2026-07-20 — 2 sort tests (folders by recursive size with pending clustered by name; unchanged without
  `sizeOf`). `npm run check` clean; full frontend suite **891 green** (i18n 100%-coverage guard passes).

## Resolution
Opt-in recursive folder sizes in the details Size column (reuses the existing `dir_size`; no backend
change). When on, each visible folder's subtree size is fetched lazily (`rawInvoke("dir_size")`),
per-path-cached (invalidated on navigation/refresh), rendered ("…" until it lands), and used as the size-sort
key so folders order by computed size (pending → name-stable). Off by default, so a plain listing pulls no
`dir_size` walks and the Size column is byte-for-byte unchanged. Files: `sort.ts` (`sizeOf` param),
`settings.ts` (`cpe.showFolderSizes`), `FileList.svelte` (props + cell + visible-window `needSizes`),
`ExplorerPane.svelte` (forwarding), `App.svelte` (cache/fill/toggle/sort wiring), `i18n.ts` (3 keys × 12
locales), `sort.test.ts` (+2 tests).

**GUI-verified in the sidecar dev build (CDP):** in the repo folder with the column **on**, folders showed
computed subtree sizes (`docs` 125.3 KB, `src` 1.6 MB, `Tickets` 2.0 MB, `node_modules` 115.4 MB) while files
kept their own size; clicking the **Size** header ordered the folders by recursive size ascending
(docs→src→Tickets→node_modules→src-tauri). With the column **off**, folder rows showed **blank** in the Size
column (no `dir_size` cost). All three ACs met. CPE-750 → Done.
(Observation: `dir_size` reported `src-tauri` at ~72 GB — that's the pre-existing command's value for the
multi-variant Rust `target/`, surfaced faithfully; not introduced here.)
