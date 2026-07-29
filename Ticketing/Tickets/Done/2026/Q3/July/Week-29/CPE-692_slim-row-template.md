---
id: CPE-692
title: Slim the file-list row template
type: enhancement
component: Frontend
priority: low
status: Done
created: 2026-07-18
closed: 2026-07-18
epic: CPE-688
estimate: 1-2h
---

## Summary
Child of CPE-688. Reduce per-row handlers/bindings and hoist shared work out of the row template so the
~30–50 rows on screen (post-virtualization) are cheap. Prereq: CPE-690.

## Acceptance Criteria
- [x] Fewer per-row handlers/bindings; no behaviour change; `npm run check` + suite green.

## Work Log
2026-07-18 — Picked up (dayshift, autonomous). Estimate: 1-2h. Plan: hoist per-row O(n) scans and
repeated lookups out of the FileList row template. The membership tests `cutPaths.includes(entry.path)`
and `draggedPaths.includes(entry.path)` ran once per row per render — O(rows × set) each, worst during a
drag with a large selection. `activity[entry.path]` was looked up four times per row.

2026-07-18 — The slimming is prereq-independent of CPE-690: the row-template cost reductions are pure
per-row cleanup, orthogonal to the windowing wiring, and fully verifiable headlessly (`npm run check` +
vitest). Done now rather than waiting on the GUI-verified render integration.

2026-07-18 — Changes in `src/lib/components/FileList.svelte`:
- Added `$: cutSet = new Set(cutPaths)` and `$: draggedSet = new Set(draggedPaths)`, recomputed only
  when the source arrays change. Row bindings now use `cutSet.has(...)` / `draggedSet.has(...)` — O(1)
  per row instead of an `Array.includes` scan. Removed the now-dead `isCut` helper.
- Hoisted the per-row activity lookup to `{@const act = activity[entry.path]}` and reused `act` for the
  four bindings that previously each re-indexed the map (`class:agent-active`, `data-agent-kind`, the
  `{#if}`, and the badge that read `.kind` twice).

2026-07-18 — `npm run check` clean (0 errors, 0 warnings). Full vitest suite green: 693 tests / 73 files.
No behaviour change — purely how the row template computes existing bindings.

## Resolution
Slimmed the FileList row template so each on-screen row does O(1) constant work for its cut/dragged
membership state and a single activity-map lookup, instead of two O(n) `Array.includes` scans plus four
repeated map indexings per render. Implemented with two reactive `Set`s (`cutSet`, `draggedSet`) and a
hoisted `{@const act}`; the dead `isCut` helper was removed. No behaviour change — verified by
`npm run check` and the full 693-test suite. Files: `src/lib/components/FileList.svelte`. Advances epic
CPE-688 (streaming/render liveness); the virtualization render integration (CPE-690) remains the
GUI-verified piece.
