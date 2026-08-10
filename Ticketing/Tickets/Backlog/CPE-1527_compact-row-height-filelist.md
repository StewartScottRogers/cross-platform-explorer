---
id: CPE-1527
title: "Compact density: tighter row/tile pitch in FileList (details/icons/gallery)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1488
created: 2026-08-09
---
## Context
The headline of CPE-1488: with density = `compact`, the file list should pack more rows per screen —
tighter row pitch in details view, smaller tiles/icons in icons/gallery view. `FileList.svelte` already
virtualizes on a **fixed** row/tile pitch (`let rowH = 30` per view, CPE-690/CPE-766) — only the visible
window renders, so a compact pitch is cheap (no perf cost, as the epic brief notes), but the fixed-height
virtualization math must be recomputed for the new pitch, not just re-styled with CSS, or scroll-into-view
and windowing will drift.

## Scope
- A compact row-height/tile-size variant for **all three** view modes FileList renders (`details`,
  `icons`, `gallery`): tighter `rowH`, optionally smaller icons, when `density === "compact"`.
- Keep the fixed-height-per-view invariant CPE-690/766 established — compact is still one constant
  pitch per view (not variable/measured heights); only the constant itself changes.
- Keyboard nav, selection, scroll-into-view, rename-in-place, and drag/drop must keep working unchanged
  at the new pitch (they already key off `rowH`/the windowed slice, per the existing virtualization code
  — verify, don't reimplement).
- Comfortable (default) stays pixel-identical to today — this is strictly additive.

## How
- Consume the `density` prop threaded through by CPE-1526 (prerequisite — do not re-add the setting or
  the App.svelte wiring here, it already exists).
- Adjust the `rowH` measurement/constant logic in `FileList.svelte` to branch on `density`, keeping it a
  single source of truth the virtualization window math already reads.
- No new dependency. Theme vars only for any new CSS.

## Verify
`npm run check` + `npx vitest run` covering `src/lib/components/FileList.test.ts` and
`FileList.virtualize-guard.test.ts` — extend both with density-aware cases (compact pitch value, windowed
slice recomputed correctly for the new pitch, comfortable output unchanged) rather than adding a parallel
suite. This is fully unit-testable headlessly (jsdom + the existing virtualization test harness); no GUI
verification required to land it, though it's a good gui-smoke screenshot candidate later.

## Notes
**Conflict surface:** `src/lib/components/FileList.svelte`, `src/lib/components/FileList.test.ts`,
`src/lib/components/FileList.virtualize-guard.test.ts`. **Prereq: CPE-1526** (needs the `density` prop).
Disjoint from CPE-1528's files (toolbar/tabbar/sidebar) — the Foreman can dispatch CPE-1527 and CPE-1528
to two workers in parallel once CPE-1526 has landed.
