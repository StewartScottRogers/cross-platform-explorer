---
id: CPE-1147
title: "Column-picker button: move it to the LEFT of the header + use a more indicative icon"
type: chore
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-30
epic: CPE-707
---

## Summary
User GUI-verify feedback on the shipped column-picker (CPE-1146) — the feature works great ("looks good"),
two small UI tweaks:
1. **Position:** the column-picker button currently sits at the **RIGHT** end of the file-list header row
   (`FileList.svelte`, the `.columns-btn` after the `{#each activeMetaColumns}` block — it auto-places into the
   trailing `1fr` spacer track). The user wants it on the **LEFT** side of the middle pane (the file-list
   header), not the right.
2. **Icon:** it currently uses `<Icon name="details" size={13} />`, which reads as a list/details-view toggle,
   not "choose columns." Use a **more indicative** glyph.

## Design
- **Move the button to the left of the header** WITHOUT breaking header↔row column alignment or the CPE-1140
  pane-min work. Options (pick the cleanest that keeps the grid honest):
  - Place `.columns-btn` as the FIRST child of the `.columns` header grid, left-aligned, so it reads as a
    leading "column options" control — but ensure it does NOT shift/overlap the Name column header or
    misalign the header grid from the file rows (the resizers align header boundaries to row columns). If a
    leading control needs its own space, prefer a small left-aligned overlay/absolute-positioned button
    within the header's left edge over adding a grid track that the file ROWS don't also have (which would
    desync header vs rows).
  - Whatever placement: the built-in + metadata column headers, their resizers, and the file rows below must
    stay perfectly aligned (verify the boundaryOffsets/resizer math still lines up), and the middle-pane
    `MID_MIN` / CPE-1140 behaviour must not regress.
- **Icon:** switch to **`settings`** (the gear — reads as "column options", fitting a picker that
  adds/removes/reorders). Only if a `settings`/gear icon already appears adjacent in the header (making two
  gears ambiguous), fall back to **`plus`** ("add column"). Keep `size={13}` + the existing `title`/`aria-label`
  (`fl.columnsButton`). No new glyph needs adding — both `settings` and `plus` exist in `Icon.svelte`.
- Keep `data-testid="open-column-picker"` (the gui-smoke pin / tests key off it) and the `openColumnPicker`
  dispatch unchanged.

## Acceptance Criteria
- [x] The column-picker button renders at the LEFT of the file-list header (middle pane), not the right.
- [x] Header column labels + resizers stay aligned with the file rows below; no regression to column resize,
      `boundaryOffsets`, or the CPE-1140 middle-pane minimum.
- [x] The button uses a more indicative icon (`settings` gear, or `plus` if a gear is already adjacent);
      `title`/`aria-label`/`data-testid` unchanged.
- [x] `npm run check` green; existing `FileList.test.ts` (incl. the `open-column-picker` assertions) still
      passes; the `gui-smoke` `organize`/`instant-search`/`batch-media`/column pins unaffected (the testid is
      unchanged).
- [x] GUI-verified on the real build (button is left-of-header + the icon reads clearly). **Deferred to the
      Foreman + user pass.**

## Work Log

- **Placement approach:** absolutely-positioned `.columns-btn` pinned to the header's left edge (`.columns`
  is already `position: sticky`, which establishes a containing block for `position: absolute` descendants —
  no new `position: relative` needed). The button was moved to be the FIRST DOM child of `.columns`, ahead of
  the `{#each COLUMNS}` block, but because it's `position: absolute` it is taken OUT of grid flow entirely —
  it consumes zero grid tracks. This means `colTemplate` (`columnsTemplate(allWidths)` from `columns.ts`),
  `boundaryOffsets`, and the `.row` grid (which shares the same `--filelist-cols` template) are completely
  untouched — verified by reading `columns.ts` (`columnsTemplate`/`boundaryOffsets` take only the real column
  widths array, never see the button) and confirming the resize `<span class="col-resize">` handles are
  already excluded from grid flow the same way (comment at their definition: "`.columns` is position:sticky,
  so these absolute handles are contained by it"). The Name header button gets `class:name={col.key ===
  "name"}` → new CSS rule `.col.name { padding-left: 34px; }` so its label/chevron clears the 24px button
  (positioned at `left: 4px`), leaving a ~6px gap. `MID_MIN` (`columns.ts`) is derived purely from
  `COLUMN_MINS` + `FILELIST_CHROME`, neither of which changed, so CPE-1140's middle-pane minimum is
  unaffected.
- **Icon:** `settings` (gear) — grepped the whole `src/` tree for `name="settings"` and found zero existing
  usages, so no adjacent-gear ambiguity in this header (or anywhere in the app); used the ticket's default
  choice instead of the `plus` fallback.
- **Verify:** `npm run check` → 0 errors, 0 warnings. `npx vitest run src/lib/components/FileList.test.ts` →
  28/28 passed (the `open-column-picker` click-dispatch test asserts only the testid + event, not position/
  icon, so it needed no changes). `npx vitest run` full suite → 126 files / 1402 tests passed, nothing else
  broke.

## Notes
- Pure cosmetic follow-up to CPE-1146; the feature (dynamic columns, type-aware sort, per-folder persistence)
  is already GUI-verified working. Small, FileList.svelte-local change.
