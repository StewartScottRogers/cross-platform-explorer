---
id: CPE-1147
title: "Column-picker button: move it to the LEFT of the header + use a more indicative icon"
type: chore
component: Frontend
priority: medium
status: Backlog
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
- [ ] The column-picker button renders at the LEFT of the file-list header (middle pane), not the right.
- [ ] Header column labels + resizers stay aligned with the file rows below; no regression to column resize,
      `boundaryOffsets`, or the CPE-1140 middle-pane minimum.
- [ ] The button uses a more indicative icon (`settings` gear, or `plus` if a gear is already adjacent);
      `title`/`aria-label`/`data-testid` unchanged.
- [ ] `npm run check` green; existing `FileList.test.ts` (incl. the `open-column-picker` assertions) still
      passes; the `gui-smoke` `organize`/`instant-search`/`batch-media`/column pins unaffected (the testid is
      unchanged).
- [ ] GUI-verified on the real build (button is left-of-header + the icon reads clearly). **Deferred to the
      Foreman + user pass.**

## Notes
- Pure cosmetic follow-up to CPE-1146; the feature (dynamic columns, type-aware sort, per-folder persistence)
  is already GUI-verified working. Small, FileList.svelte-local change.
