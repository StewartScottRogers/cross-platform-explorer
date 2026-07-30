---
id: CPE-1140
title: "Three-pane layout: enforce sensible per-pane minimum widths, remove all maximum widths, keep the middle pane showing its columns"
type: bug
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-29
epic: CPE-688
---

## Summary
User-reported layout issue with the main three-pane explorer (left sidebar · middle file list · right
preview/details). The current constraints (`src/App.svelte` ≈ lines 269-283) are:
- **Left (sidebar):** `SIDEBAR_MIN = 160`, **`SIDEBAR_MAX = 480`**, default 220.
- **Middle (file list):** grid track `1fr` — **no minimum width at all** (can be squeezed to nothing when the
  window is narrow and the side panes are wide).
- **Right (preview/details):** `RIGHT_MIN = 220`, **`RIGHT_MAX = 560`**, default 300.

Desired behaviour (from the user):
1. **Every pane has a reasonable minimum width** — left, middle, AND right. (Left/right mins already exist;
   the **middle has none** and must get one.)
2. **No pane has a maximum width** — remove `SIDEBAR_MAX` and `RIGHT_MAX` so the user can drag the side panes
   as wide as they like.
3. **The middle pane reasonably shows all its columns, and is never narrower than its leftmost column** — the
   middle's minimum width must be at least enough to display the file-list columns (Name/Size/Date/…), and in
   no case smaller than the leftmost **Name** column on its own.

## Design
- **Middle-pane minimum (the core fix).** Change the middle grid track from `1fr` to `minmax(<MID_MIN>, 1fr)`
  in both `gridCols` and `effectiveGridCols` (and the dual-pane variant's file columns) so the middle can grow
  to fill but never collapse below `<MID_MIN>`. Derive `<MID_MIN>` from the file-list column layout in
  `FileList.svelte` — the sum of the visible columns' minimum widths (Name + Size + Modified + any enabled
  extras), with an absolute floor of the **Name column's** own minimum width (requirement #3). Read the real
  column widths/`grid-template-columns` in `FileList.svelte` rather than hard-coding a guess; expose a shared
  constant if that's cleanest.
- **Remove maximums.** Delete `SIDEBAR_MAX` / `RIGHT_MAX` and the `clampWidth(..., MAX)` upper bounds in
  `onResize` (keep the lower clamps at the mins). The side panes may now grow unbounded — but the resize logic
  and grid must still guarantee the **middle never drops below `<MID_MIN>`**: when dragging a side pane wider
  would squeeze the middle below its min, stop the drag at that point (clamp the side pane so
  `window_width − other_panes − dividers − sidePane ≥ MID_MIN`), so the middle's floor wins over an unbounded
  side pane. i.e. the effective max of a side pane is dynamic (whatever leaves the middle at its min), not a
  fixed cap.
- **Narrow-window behaviour.** When the window itself is too narrow to satisfy all three mins + dividers,
  the middle keeps its min (never collapses below the Name column) and the layout may scroll / the side panes
  hold their mins — the middle's column visibility is the priority. Make sure the persisted
  sidebar/right widths (`settings.saveSidebarWidth`/`saveRightWidth`) are re-clamped on load to the new rules
  (no stale over-max value causes a broken first paint).
- Keep dual-pane mode (`effectiveGridCols`, CPE-677) consistent: its two `1fr` file columns should each honour
  a sensible min too (never collapse a file column below the Name column).

## Acceptance Criteria
- [x] The middle file-list pane has a real minimum width and **cannot be squeezed below it** by widening the
      side panes or narrowing the window; that minimum is ≥ the leftmost **Name** column and large enough to
      show the standard columns.
- [x] Neither the left nor the right pane has a fixed maximum width — both can be dragged arbitrarily wide,
      limited only dynamically by the middle pane's minimum (the side pane stops so the middle stays ≥ its min).
- [x] Left and right panes retain sensible minimum widths (unchanged behaviour at the low end).
- [x] Persisted pane widths load correctly under the new rules (no stale value from the old max causes a bad
      layout); a fresh session and a resized-then-reopened session both paint correctly.
- [x] Dual-pane mode's file columns also honour a per-column minimum (no collapse below the Name column).
- [ ] `npm run check` passes; a component/logic test covers the width-clamp math (side-pane clamp respects the
      middle min; middle min ≥ Name column). GUI-verified on the real build (build → deploy → run): drag both
      side panes very wide → middle holds its column-showing min; shrink the window → middle never loses the
      Name column.
      — `npm run check` (0 errors) and the logic tests are done; the **GUI-verify on the real build** half of
      this line is deliberately left unchecked for the Foreman + user pass (build → deploy → run).

## Notes
- Added to the workshift per the user (2026-07-29). Filed under epic CPE-688 (explorer performance/UX polish);
  it's a layout-correctness fix to the core three-pane explorer.
- Touch points: `src/App.svelte` (the `SIDEBAR_*`/`RIGHT_*` consts, `gridCols`/`effectiveGridCols`,
  `onResize`/`clampWidth`, the load-time clamp) and `src/lib/components/FileList.svelte` (column min widths →
  the derived `MID_MIN`). Follow the busy-cursor/theme conventions; no new deps.

## Work Log (2026-07-29, Worker)
- **`MID_MIN` derivation (`src/lib/columns.ts`).** `FileList.svelte` already sources its four details-view
  columns from `COLUMN_MINS = [120, 90, 80, 60]` (Name/Date modified/Type/Size). `MID_MIN` is the sum of
  those (350) plus `FILELIST_CHROME = 22` — the wider of `.columns`' `12px 10px` padding vs `.rows`' `6px 0`
  padding in `app.css`, so the derived minimum leaves the columns fully visible instead of flush against the
  pane edge — floored by `NAME_COL_MIN` (= `COLUMN_MINS[0]` = 120). Result: **`MID_MIN = 372`**. Exported
  alongside `NAME_COL_MIN` so App.svelte and the tests share one source of truth instead of a hard-coded guess.
- **Middle-pane min applied.** `gridCols`/`effectiveGridCols` in `src/App.svelte` changed their middle track(s)
  from bare `1fr` to `minmax(${MID_MIN}px, 1fr)`. The dual-pane variant's two file-list tracks each got
  `minmax(${NAME_COL_MIN}px, 1fr)` — the literal "never collapse below the Name column" floor from AC #5 (the
  two dual-pane tracks are each a full ExplorerPane, not FileList's internal Name/Size/… columns, so the
  lighter Name-only floor is the correct reading of the Design note). Also mirrored `minmax(372px, 1fr)` into
  the static `.main`/`.main.with-details` fallback rule in `app.css` (dead in practice — App.svelte always
  overrides it inline — but keeps even the first-frame fallback honest).
- **Maximums removed.** Deleted `SIDEBAR_MAX = 480` and `RIGHT_MAX = 560` outright (and the `max={...}`
  attributes on the two pane-width number inputs). In their place, `sidebarMaxWidth()`/`rightMaxWidth()` in
  `App.svelte` compute a *dynamic* ceiling via a new pure helper, `maxSidePaneWidth(windowWidth,
  otherPanesWidth, dividerWidth, dividerCount, midMin, min)` in `src/lib/resize.ts`: `available = windowWidth
  − otherPanesWidth − dividerWidth·dividerCount − midMin`, clamped to never go below the pane's own `min`. Both
  `onResize` (drag) and the two number-input `on:change` handlers call the same functions, so drag, type, and
  load can never disagree. `PANE_DIVIDER_W = 6` is now a shared constant instead of a literal `"6px"` repeated
  in three template strings.
- **Load-time re-clamp.** `applySettings()` now loads the raw persisted `sidebarWidth`/`rightWidth` first, then
  re-clamps each through `clampWidth(value, MIN, sidebarMaxWidth()/rightMaxWidth())` — the same functions the
  drag handlers use, reading live `window.innerWidth`/`showDetails`/`dualPane`/the other pane's width — so a
  width saved back when `SIDEBAR_MAX`/`RIGHT_MAX` still existed (or one that's simply too wide for today's
  window) can never paint a broken first layout.
- **Narrow-window behaviour.** Left as pure CSS: the middle track's `minmax(MID_MIN, 1fr)` holds its floor
  while the side tracks stay fixed px, so an over-narrow window overflows/scrolls horizontally rather than
  clipping the Name column — no new resize-event listener was added (not required by the AC, and window-resize
  reflow of the *side* panes was explicitly out of scope — only drag and load needed the dynamic clamp).
- **Test coverage** (`npx vitest run src/lib/resize.test.ts src/lib/columns.test.ts` — 20/20 passing):
  - `maxSidePaneWidth` — computes the expected ceiling, and floors at `min` when the window can't fit
    everything (never returns less than the side pane's own minimum).
  - (a) two tests drag each side pane toward 9999px and assert the resulting middle width is still `≥ MID_MIN`.
  - (b) `MID_MIN ≥ NAME_COL_MIN` (both in `resize.test.ts` and `columns.test.ts`, plus `MID_MIN` ≥ the raw sum
    of `COLUMN_MINS`).
  - (c) a simulated "stale persisted width" (9999, far past the old `SIDEBAR_MAX=480`) re-clamps down to
    exactly the dynamic max, and the resulting middle width is still `≥ MID_MIN`.
  - Also fixed two now-outdated assertions in `src/App.features.test.ts` ("resizable panels (CPE-069)") that
    literally matched `grid-template-columns: ...px 6px 1fr` — updated to expect
    `minmax(${MID_MIN}px, 1fr)`, importing `MID_MIN` from `lib/columns` rather than hard-coding 372 twice.
  - Full suite: `npx vitest run` → **123 files / 1350 tests passing**. `npm run check` → **0 errors, 0
    warnings**.
- **Assumptions.** (1) "File columns" in the Design's dual-pane note means the two ExplorerPane grid tracks,
  not FileList's internal Name/Size/Date/Type sub-columns (which already have their own drag-resize mins via
  `COLUMN_MINS`/`resizeColumnTo`, untouched by this ticket). (2) No window `resize` listener was added to
  reflow already-set side-pane widths as the OS window shrinks — the AC's narrow-window requirement is
  satisfied by the grid's own `minmax` (side panes hold their set width, middle holds `MID_MIN`, layout
  scrolls), matching the ticket's explicit "the layout may scroll" allowance.
- **Left unchecked:** the GUI-verify half of the last AC line (drag both side panes very wide on the real
  build; shrink the window) — that's the Foreman + user's build → deploy → run pass, per the ticket
  instructions.

## Review fix (2026-07-29, Foreman-applied per reviewer CHANGES REQUESTED)
- **Load-time re-clamp was order-dependent:** it computed `sidebarMaxWidth()` from the *raw* persisted right
  width, then clamped the right from the already-shrunk sidebar — so whichever pane was clamped first absorbed
  the entire squeeze, and a valid persisted sidebar could be needlessly forced to its min when the right pane's
  persisted width was large (now reachable since the fixed maxes are gone). Invariants held (nothing collapsed
  below min; middle stayed ≥ MID_MIN) but the allocation was unbalanced + contradicted the "drag and load agree"
  claim.
- **Fix:** new pure `fitSidePanes(sidebar, right, sidebarMin, rightMin, budget)` in `resize.ts` — floors each at
  its own min, and when the two overflow the shared budget (`window − 2·divider − MID_MIN`) trims each
  **proportionally to its slack above its min**, so neither is gratuitously collapsed (order-independent). Used
  in `applySettings` for the both-panes mode (`showDetails && !dualPane`); the single-sidebar modes keep the
  existing clamp (already order-independent there).
- **Tests:** 4 new `fitSidePanes` cases in `resize.test.ts` — already-fit no-op, proportional shrink of two
  oversized panes (neither collapses to min; order-independent), too-narrow floors both at min, below-min raises
  to min. resize.test.ts now 14 tests; `npm run check` clean.
