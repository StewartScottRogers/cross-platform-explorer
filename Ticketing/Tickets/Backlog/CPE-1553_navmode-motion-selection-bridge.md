---
id: CPE-1553
title: "Navigation Mode: motion/visual-select bridge onto the existing selection engine (pure logic)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1487
created: 2026-08-10
---
## Context
CPE-1552 lands `src/lib/navMode.ts`'s pure reducer, which turns keystrokes into `NavIntent` values
(`motion`, `enterVisual`/`exitVisual`, …) but knows nothing about the file list itself. CPE-1487's brief is
explicit that visual-range multi-select must build **on top of CPE-711's selection engine, not reimplement
it** — `src/lib/selection.ts`'s `Selection { indices, anchor, lead }` plus `click(sel, index, {ctrl,
shift})`, `moveLead`, `selectAll`, `selectIndices`, `isSelected` are the same primitives the mouse and the
existing arrow-key handling already use, and `src/lib/gridnav.ts`'s `arrowDelta`/`pageDelta` already
convert a direction into an index delta for the current layout (list vs. grid).

This ticket is the translation layer between a `NavIntent` and a `Selection` — still pure logic, still no
`App.svelte` edit, so it stays independently testable and mergeable in parallel with the other new-file
tickets in this batch.

## Scope
- New file `src/lib/navMotion.ts`:
  - `export function applyNavIntent(intent: NavIntent, sel: Selection, itemCount: number, mode: NavMode,
    layout: "list" | "grid", columns?: number): Selection` (import `NavIntent`/`NavMode` from
    `./navMode`, `Selection`/`click`/`moveLead`/`selectAll`/`selectIndices` from `./selection`,
    `arrowDelta`/`pageDelta` from `./gridnav`).
    - `motion` intents: resolve `dir` + `layout`/`columns` to an index delta via the existing
      `arrowDelta` helper (reused, not reimplemented) for `left`/`down`/`up`/`right`; `top`/`bottom` jump
      directly to index `0`/`itemCount - 1`. Multiply the single-step delta by `intent.count`, clamp the
      resulting index to `[0, itemCount - 1]`.
    - In `mode === "normal"`: the new index becomes the sole selection (equivalent to a plain click —
      call `click(sel, newIndex, { ctrl: false, shift: false })` or `selectIndices([newIndex])`, whichever
      `selection.ts`'s existing API makes idiomatic; do not hand-roll set logic here).
    - In `mode === "visual"`: the new index extends the range from the existing `anchor` (equivalent to a
      shift-click — call `click(sel, newIndex, { ctrl: false, shift: true })`), so `v` + repeated motions
      grows/shrinks a contiguous range exactly like today's Shift+Arrow does.
    - `itemCount === 0`: return `sel` unchanged (no-op on an empty pane).
    - Non-motion intents (`op`, `enterVisual`/`exitVisual`, `startFilter`, `startCommand`, `none`) are out
      of scope for this function — they return `sel` unchanged; CPE-1556 dispatches those directly to the
      existing `doCopy`/`doCut`/`doPaste` etc. functions, not through this helper.
  - No edits to `selection.ts` or `gridnav.ts` — both are imported read-only.

## How
New `src/lib/navMotion.test.ts` (pure, no DOM, table-driven like `src/lib/selection.test.ts`): each
direction in `mode:"normal"` replaces the selection with a single new index; the same directions in
`mode:"visual"` extend the range from a fixed anchor (assert both endpoints, not just the lead); `top`/
`bottom` jump to `0`/`itemCount-1` in both modes; a `count` of e.g. `3` on `down` moves 3 rows in one
call; clamping at both list boundaries (moving `up` from index 0, `down` from the last index) is a no-op
past the edge rather than wrapping or throwing; `itemCount === 0` returns the input `sel` reference
unchanged; a non-motion intent (e.g. `{kind:"op",op:"yank"}`) returns `sel` unchanged.

## Verify
`npx vitest run src/lib/navMotion.test.ts`; `npm run check`. Fully headless — pure TS logic over
`Selection`/`NavIntent` value objects, no DOM, no Tauri invoke.

## Notes
**Conflict surface:** two new files only (`src/lib/navMotion.ts`, `src/lib/navMotion.test.ts`). Imports
(read-only, no edits) from `src/lib/navMode.ts` (CPE-1552), `src/lib/selection.ts`, and
`src/lib/gridnav.ts`. No overlap with CPE-1554 or CPE-1555's files. **Dispatch order:** after CPE-1552
(needs its `NavIntent`/`NavMode` exports); independent of, and mergeable in parallel with, CPE-1554 and
CPE-1555.
