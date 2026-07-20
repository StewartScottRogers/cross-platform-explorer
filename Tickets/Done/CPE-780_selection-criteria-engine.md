---
id: CPE-780
title: Pure selection-criteria engine (Select by…)
type: feature
status: Done
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed: 2026-07-20
epic: CPE-711
estimate: 1h
---

## Summary
Foundation for advanced selection (epic CPE-711). A pure module (`src/lib/selectMatch.ts`) that returns the
indices of entries matching a criteria, reusing the CPE-774 `Condition` model (no parallel matcher) — so the
"Select by…" dialog (CPE-782) is a thin wire into the selection.

## Scope
- `selectMatching(entries: DirEntry[], condition: Condition, now: number): number[]` — indices where
  `matchesCondition` (CPE-774) is true.
- `sameExtensionAs(entries, seed): number[]` — extend a seed selection to every file sharing an extension
  with any seed file (the "select same type" power move).
- Pure + total (empty entries, seed with dirs / out-of-range indices).

## Acceptance Criteria
- [x] `selectMatching` returns exactly the matching indices for each `Condition` kind.
- [x] `sameExtensionAs` unions all files matching any seed file's extension; ignores dirs / bad indices.
- [x] Pure + dependency-light; unit tests cover both; `npm run check` + suite green.

## Notes
Reuses `src/lib/colorRules.ts` (CPE-774). Foundation for CPE-782. Headless.

## Resolution
Added `src/lib/selectMatch.ts` (pure): `selectMatching(entries, condition, now)` → matching indices via the
CPE-774 `matchesCondition` (no parallel matcher), and `sameExtensionAs(entries, seed)` → all files sharing
an extension with any seed file (dirs / extension-less / out-of-range seeds ignored; sorted). 4 tests. check
0/0. Headless; reuses colorRules. Foundation for CPE-782.

