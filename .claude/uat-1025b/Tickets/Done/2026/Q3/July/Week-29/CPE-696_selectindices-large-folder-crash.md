---
id: CPE-696
title: selectIndices crashes on large folders (Math.min/max spread over N indices)
type: bug
component: Frontend
priority: high
created: 2026-07-18
status: Done
closed: 2026-07-18
epic: CPE-688
estimate: 45m
---

## Summary
`selectIndices` (`src/lib/selection.ts`) computes the anchor/lead with `Math.min(...clean)` /
`Math.max(...clean)`. Spreading a large array into a function argument list hits V8's argument-count limit
and throws **`RangeError: Maximum call stack size exceeded`** (confirmed: `Math.min(...arrayOf200k)`
throws). Both the **"Invert selection"** action (`invertSelection` → `selectIndices` over `visible.length`)
and **"Select all of this type"** (`selectIndices(sameTypeIndices(…))`) feed ~N indices in, so on a large
folder — exactly the case the perf epic (CPE-688) targets — inverting or type-selecting **crashes**.

Fix: compute min/max with an O(n) loop (no spread). Pure logic, unit-testable, no GUI.

## Acceptance Criteria
- [x] `selectIndices` no longer spreads the index array into `Math.min`/`Math.max`.
- [x] A test builds a selection over a very large index array (≥200k) without throwing, with correct
      anchor/lead/count.
- [x] Existing selection behaviour is unchanged (all selection tests pass).
- [x] `npm run check` + full suite green.

## Work Log
2026-07-18 (nightshift) — Picked up. Estimate: 45m. Waterfall at "create new work". Confirmed the crash
empirically: `node -e "Math.min(...Array.from({length:200000}))"` → "Maximum call stack size exceeded".
Traced the two large-array feeders: `invertSelection` (context action, over `visible.length`) and
"Select all of this type" (`selectIndices(sameTypeIndices(...))`).

2026-07-18 — Fixed `selectIndices` to derive anchor/lead with an O(n) loop instead of
`Math.min(...clean)`/`Math.max(...clean)`. Added two regression tests (200k selectIndices; 150k
invertSelection) that would throw `RangeError` under the old code. `npm run check` clean; full suite green:
701 tests / 74 files (+2).

## Resolution
`selectIndices` used `Math.min(...clean)`/`Math.max(...clean)`, which throws
`RangeError: Maximum call stack size exceeded` when `clean` is large enough to exceed V8's argument-count
limit — so "Invert selection" and "Select all of this type" **crashed** on large folders (the exact case
CPE-688 targets). Replaced the spread with an O(n) min/max loop; behaviour is otherwise identical (existing
selection tests unchanged). A latent crash and a (minor) perf win on the same path. Files:
`src/lib/selection.ts`, `src/lib/selection.test.ts`. Advances epic CPE-688.
