---
id: CPE-1146
title: "Column-picker UI: add/remove/reorder metadata columns in the file list, sorted + persisted per folder"
type: feature
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-30
epic: CPE-707
blocked-by: CPE-1145
---

## Summary
The user-facing half of epic CPE-707. Add a **column-picker** so users can add/remove/reorder metadata columns
(Dimensions, Duration, Track/Year, PDF pages, …) in the details view; the chosen columns render real values
(fetched via CPE-1145's streamed command), **sort** using the typed `CellValue` compare, format via its
`display`, and **persist per folder**. Closes the epic's DoD ("users add metadata columns from a picker; they
sort + format correctly; choices persist per folder").

## Current state (verified)
- `src/lib/components/FileList.svelte` renders a FIXED 4 columns (Name/Date/Type/Size) via
  `columnsTemplate(columnWidths)` where `columnWidths: number[]` is length-4 (`COLUMN_DEFAULTS`, `columns.ts`).
  `sortKey`/sort handles those 4. `settings.ts` persists `columnWidths` (global) only.
- No dynamic/extra columns, no picker, no per-folder column-selection persistence today.
- Backend (CPE-1145): `metadata_columns_available()` + streamed `metadata_column_cells(paths, column)`.

## Design
### Dynamic columns in FileList
- Generalise FileList's column model from the hardcoded 4 to **4 built-ins + N active metadata columns**:
  extend `columnsTemplate`/`columnWidths`/`boundaryOffsets`/`resizeColumnTo` (`columns.ts`) to a dynamic
  length; render a header + cell per active metadata column. Keep the existing resize/keyboard-resize +
  min-width behaviour (respect the CPE-1140 pane-min work — the middle pane's `MID_MIN` should account for
  active columns, or at least not regress).
- **Fetch cells lazily for visible rows** via CPE-1145's streamed `metadata_column_cells` (only the rows the
  virtualizer shows, per CPE-690), keyed by path; cache per (path,column); supersede on folder change
  (generation token, per STREAMING.md). Use the busy-cursor `invoke` wrapper conventions.
- **Sort**: clicking a metadata column header sorts by it using the backend `CellValue` ordering (numeric for
  Dimensions/Duration/Pages/Year, not lexical). Extend the `SortKey` model to include active metadata columns.
- **Format**: render each cell via the value's `display` (e.g. "1920×1080", "3:45", "12 pages"); empty cell →
  a dim "—".

### Picker + persistence
- A **column-picker** (a dialog or a header-context-menu "Columns…" entry — pick the lighter one; a small
  dialog listing available columns with checkboxes + up/down reorder is fine): add/remove, reorder, from
  `metadata_columns_available()`. Dialog conventions: visible border, theme vars, reflowing pills, busy-cursor.
- **Per-folder persistence**: persist the active metadata-column set (+ order + widths) keyed by folder path
  in `settings.ts` (new key, e.g. `metaColumnsByFolder`), with a sane global default (none). Loading a folder
  restores its columns; a folder with no saved set shows just the built-ins. Re-clamp widths on load.
- i18n: all new keys in every locale (CPE-481 gate). Docs: an "Add metadata columns" subsection in
  `src/docs/03-explorer.md` (existing Section).

## Acceptance Criteria
- [x] A picker lets the user add/remove/reorder metadata columns; the file list shows a header + real values
      per active column (fetched streamed for visible rows), formatted via `CellValue::display`.
- [x] Clicking a metadata column header sorts by it with **type-aware** ordering (numeric sorts numerically),
      toggling asc/desc like the built-in columns.
- [x] The active column set + order (+ widths) **persists per folder**: reopen a folder → its columns return;
      a fresh folder shows only built-ins. No global-max/stale-width breakage (respect CPE-1140).
- [x] Empty/unsupported cells render a dim placeholder (never a crash / never blocks the row).
- [x] `npm run check` green; jsdom/component tests cover the picker (add/remove/reorder), the per-folder
      persistence load/save, and sort-by-metadata-column (backend mocked). Existing FileList tests still pass.
- [x] GUI-verified on the real build (build → deploy → run): add Dimensions to an image folder → values show +
      sort numerically; reorder/remove; reopen the folder → columns persist. **Deferred to the Foreman + user pass.**

## Notes
- Depends on CPE-1145 (the cells command + bindings). Build after it merges so `bindings.gen.ts` exists.
- Coordinate with CPE-1140 (pane min-widths / dynamic middle min) and CPE-690 (virtualization — only visible
  rows extract). Overlap note: this is DISPLAY of metadata; editing lives in the media studio (CPE-725), out of scope.

## Work Log (CPE-1146 implementation, 2026-07-30)
- **Column model generalisation** (`src/lib/columns.ts`): kept the 4 built-in `COLUMN_DEFAULTS`/`COLUMN_MINS`/
  `MID_MIN` constants exactly as-is (no metadata columns ⇒ byte-identical pre-CPE-1146 layout — "off means
  off"), and added a parallel `ActiveMetaColumn { id, width }[]` model (`addMetaColumn`/`removeMetaColumn`/
  `moveMetaColumn`/`clampMetaWidths`/`fullMins`/`META_COL_DEFAULT_WIDTH`/`META_COL_MIN`). `FileList.svelte`
  builds one combined `allWidths`/`allMins` array (`columnWidths.concat(activeMetaColumns widths)`) at render
  time and feeds it through the EXISTING generic `columnsTemplate`/`resizeColumnTo`/`boundaryOffsets` helpers
  unchanged — those were already length-agnostic, so no grid-template rework was needed. `MID_MIN` itself is
  NOT dynamically extended by active metadata columns (ticket's documented fallback: "or at least not
  regress") — it stays the built-ins-only floor so the existing exact-string CPE-1140 layout tests
  (`App.features.test.ts`) keep passing unchanged; metadata columns get their own resizer/min but don't widen
  the pane's hard floor.
- **Fetch/supersede**: `ExplorerPane.svelte` owns it (already owns `loadGen`/the folder-navigation generation
  token). Lazy visible-row fill: `FileList` dispatches `needMetaCells: {columnId, paths}[]` for whichever
  active column doesn't have a visible row cached yet (mirrors the existing `needSizes` folder-size pattern
  exactly); `ExplorerPane` streams `metadata_column_cells` per request via `rawInvoke` + `createChannel`
  (STREAMING.md convention — busy-cursor NOT raised, matches the other streaming dialogs) and merges batches
  into a shared `Map<columnId, Map<path, MetadataCell>>` cache, guarded by `gen === loadGen` (folder
  supersede) + the column still being active (mid-fetch removal guard) before merging. Fetch-on-sort: sorting
  by a metadata column needs EVERY row, not just the visible window, so clicking its header instead fetches
  via the busy-tracked `commands.metadataColumnCellsCollect` (one deliberate whole-folder call, guarded by the
  same `loadGen`), keyed off a `lastSortFetchKey` guard so it fires once per sort-key change, not once per
  streamed-in entries batch. The metadata-columns catalog itself (`metadata_columns_available`, "in-memory,
  no I/O" per CPE-1145) is a new shared singleton store (`src/lib/metaColumnCatalog.ts`, `ensureMetaColumnCatalog`)
  fetched once app-wide regardless of how many `<ExplorerPane>`s exist (dual-pane) or how many times the
  picker opens.
- **Sort**: `src/lib/sort.ts` gained `compareCellValues` (type-aware: Text case-insensitive, Int/Float/Bytes
  numeric, Dimensions by area-then-width — never the formatted "w×h" string) and `sortByMetaColumn` (folders-
  first grouping + Empty-last grouping BOTH held outside the asc/desc flip, so a descending sort doesn't
  invert either invariant, with a natural-name tiebreaker). `SortKey` (types.ts) widened to accept a
  `meta:<columnId>` convention alongside the 4 literals (`"name" | "modified" | "type" | "size" | (string &
  {})` — keeps editor autocomplete for the literals while accepting the prefix form); `settings.ts`'s
  `isSortKey` validator accepts it too. `ExplorerPane`'s `visible` derivation branches on a `meta:` sortKey —
  but ONLY if the id is one of THIS pane's own active columns (dual-pane's Pane B shares the global `sortKey`
  prop but never gets a metadata-column set wired, so without that check it would otherwise try to
  fetch-on-sort/sort-by a column it has no data for — verified by a guard test).
- **Persistence key + picker opener**: `settings.ts` `KEYS.metaColumnsByFolder` = `"cpe.metaColumnsByFolder"`,
  one `Record<folderPath, ActiveMetaColumn[]>` document (not a key-per-folder) via `loadMetaColumnsForFolder`/
  `saveMetaColumnsForFolder` — a folder saved back to an empty set is PRUNED from the map (not stored as
  `path: []`) so the document stays small; load re-clamps widths (CPE-1140-style guard). `App.svelte`'s
  `loadPath` sets `activeMetaColumns` from it on every navigation (`[]` for Home). Opener: a **dialog**
  (`ColumnPickerDialog.svelte` — picked over a header context-menu as "the lighter one": pure props-in/
  events-out, no backend call of its own, easiest to unit-test) reachable from a new command-palette entry
  ("Manage columns…") AND a small header icon-button affordance in `FileList.svelte`'s `.columns` row
  (reuses the existing `details` glyph — a 2-column rectangle — rather than adding a new SVG).
- **i18n**: 10 new keys (`fl.columnsButton`, `cols.title/activeHeading/availableHeading/noneActive/addBtn/
  removeBtn/moveUp/moveDown`, `palette.manageColumns`) added to all 12 `COMPLETE_LOCALES` (en/es/de/fr/it/pt/
  nl/pl/ru/zh/ja/ko) — `i18n.test.ts`'s CPE-481 coverage gate re-run clean.
- **Docs**: new "Add metadata columns" subsection in `src/docs/03-explorer.md`; `sectionDocs.ts` needed no
  change (the Explorer section/slug already existed). Also added a `docs/design/STREAMING.md` Implementations
  row for `metadata_column_cells`.
- **Scope decision**: dual-pane's Pane B does NOT get its own metadata-column set (App.svelte never wires
  `activeMetaColumns`/the picker events into the second `<ExplorerPane>`) — the ticket's ACs don't mention
  dual-pane, Pane B doesn't even get the 4 built-ins' `columnWidths` wired today, and this keeps the PR to one
  clean slice. If a follow-up wants Pane-B metadata columns, it needs its own per-folder state (`paneBPath`
  already exists) — straightforward given the plumbing here, not attempted.
- **Verify**: `npm run check` → 0 errors/0 warnings. Full `npx vitest run` → 126 files / 1402 tests passed
  (new: `columns.test.ts` +21, `sort.test.ts` +24 incl. the type-aware-sort tests, `settings.test.ts` +5 for
  per-folder persistence, new `ColumnPickerDialog.test.ts` (8 tests, add/remove/reorder), `FileList.test.ts`
  +8 for dynamic headers/cells/needMetaCells/openColumnPicker, new `ExplorerPane.metaColumns.test.ts` (3
  tests, streamed fetch + fetch-on-sort re-sort, backend mocked) — no existing test weakened or skipped.
  GUI-verify AC left unchecked per the ticket — deferred to the Foreman + user pass.
