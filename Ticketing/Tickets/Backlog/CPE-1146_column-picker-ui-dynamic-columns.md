---
id: CPE-1146
title: "Column-picker UI: add/remove/reorder metadata columns in the file list, sorted + persisted per folder"
type: feature
component: Frontend
priority: high
status: Backlog
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
- [ ] A picker lets the user add/remove/reorder metadata columns; the file list shows a header + real values
      per active column (fetched streamed for visible rows), formatted via `CellValue::display`.
- [ ] Clicking a metadata column header sorts by it with **type-aware** ordering (numeric sorts numerically),
      toggling asc/desc like the built-in columns.
- [ ] The active column set + order (+ widths) **persists per folder**: reopen a folder → its columns return;
      a fresh folder shows only built-ins. No global-max/stale-width breakage (respect CPE-1140).
- [ ] Empty/unsupported cells render a dim placeholder (never a crash / never blocks the row).
- [ ] `npm run check` green; jsdom/component tests cover the picker (add/remove/reorder), the per-folder
      persistence load/save, and sort-by-metadata-column (backend mocked). Existing FileList tests still pass.
- [ ] GUI-verified on the real build (build → deploy → run): add Dimensions to an image folder → values show +
      sort numerically; reorder/remove; reopen the folder → columns persist. **Deferred to the Foreman + user pass.**

## Notes
- Depends on CPE-1145 (the cells command + bindings). Build after it merges so `bindings.gen.ts` exists.
- Coordinate with CPE-1140 (pane min-widths / dynamic middle min) and CPE-690 (virtualization — only visible
  rows extract). Overlap note: this is DISPLAY of metadata; editing lives in the media studio (CPE-725), out of scope.
