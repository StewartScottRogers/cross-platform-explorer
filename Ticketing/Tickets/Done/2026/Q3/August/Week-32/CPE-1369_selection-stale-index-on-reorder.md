---
id: CPE-1369
title: "Selection operates on the WRONG file after an in-place sort/filter (stale row indices)"
type: Bug
status: Done
priority: High
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
closed: 2026-08-06
---

## Problem (found by the selection-model adversarial audit) — data-integrity

`selection` is a Set of ROW INDICES into the derived `visible` list. When `visible` re-orders/re-filters
IN PLACE — a **sort** (column header OR CommandBar), a **type/tag filter** toggle, or a **streamed batch**
appended mid-selection — WITHOUT a folder reload, the indices weren't remapped. So the highlight AND every
op target (Delete / F2 rename / Ctrl-X/C / duplicate / extract / tags / batch ops / palette commands)
silently jumped to a DIFFERENT file, with no "selection lost" cue.

Repro: select `report.pdf`; click the **Size** column header to re-sort; press **Delete** → deletes the
file now sitting at the old index, not `report.pdf`. A harmless-looking sort click silently retargets a
destructive op. The codebase already documented this exact class for Replay mode (ExplorerPane's
`selectedEntries` note) but only fixed it there.

## Fix

At the `visible` derivation in `ExplorerPane.svelte`, reconcile the selection to the new order on every
`visible` change: `$: reconcileSelectionToVisible(visible)` recovers the selected PATHS from the PREVIOUS
`visible` and `remapByPath`s them to the new indices, assigning only when the index Set actually changes.
Depends solely on `visible` (reads of `selection`/`prevVisible` live in the called helper, so no self-loop).
Covers both sort sources, filters, and streaming. Navigation is unaffected — App clears `selection` before
a real load, so there's nothing to remap; its keepSelection remap computes the identical result.

Bonus: when a filter hides the selected file, `remapByPath` returns an empty selection — strictly safer
than silently retargeting. Serves both dual-panes (each `ExplorerPane` owns its `prevVisible`).

## Tests + review

NEW `src/App.selectionReorder.test.ts` drives the real App through the exact trap (select `c.txt`, sort by
Size so it moves 2→1, assert the selection still resolves to `c.txt`) — validated FAIL-without / PASS-with.
Full frontend suite 2104/2104 green; `npm run check` clean. Independently reviewed: **APPROVE** — all six
enumerated risks (loop, navigation, stale prevVisible, replay, anchor/lead, selectedEntries ordering)
verified safe.

## Related findings (filed separately from the same audit)

- Dual-pane keyboard nav / destructive keys act on the LEFT pane regardless of active pane — CPE-1370.
- Ctrl+A / invert / select-all-of-type yank the viewport to the last row — CPE-1373.
- PageUp / PageDown don't move the selection lead — CPE-1374.

## Work Log

- 2026-08-06 — Selection-model audit surfaced this HIGH data-integrity bug. Fixed at the ExplorerPane
  visible-derivation chokepoint; validated regression test + independent review (APPROVE). Bundles into
  the next build.
