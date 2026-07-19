---
id: CPE-694
title: Cache the type sort key (decorate-sort) so a type sort stops recomputing typeName per comparison
type: enhancement
component: Frontend
priority: medium
status: Done
created: 2026-07-18
closed: 2026-07-18
epic: CPE-688
estimate: 1h
---

## Summary
Child of CPE-688 (10× file-list perf). `compareEntries` calls `typeName(entry)` on **both** operands of
**every** comparison when sorting by type. A sort is O(n log n) comparisons, so a 10k-entry folder makes
~260k `typeName` calls — and `typeName` allocates a fresh template string (`` `${ext.toUpperCase()} File` ``)
for every extension not in the lookup table, which is real GC pressure on the hot sort path the epic
targets. Precompute each entry's type key **once** (O(n)) and compare the cached values.

Headless and behaviour-preserving: pure logic in `src/lib/sort.ts`, verified by `npm run check` + the
existing/extended `sort.test.ts`. No GUI needed (this is the sort *computation*, not the render), so it is
safe to land unattended — unlike the virtualization render work (CPE-690).

## Acceptance Criteria
- [x] A type sort computes `typeName` O(n) times, not O(n log n) — verified by a test that counts calls.
- [x] Sort order is byte-for-byte unchanged (folders-first, tiebreakers, asc/desc) — existing sort tests pass.
- [x] `compareEntries` stays backward-compatible for its external callers.
- [x] `npm run check` + full suite green.

## Work Log
2026-07-18 (nightshift) — Picked up. Estimate: 1h. Waterfall reached "create new work" (Doing/Backlog all
attended-gated, no Proposed epics); filed + built this headless-safe child of the High-priority perf epic.

2026-07-18 — Implemented in `src/lib/sort.ts`:
- Added an optional `typeNameOf: (e) => string = typeName` param to `compareEntries` — default preserves the
  current behaviour for its external callers (tests), so it's backward-compatible.
- `sortEntries`, for the `type` key only, precomputes each entry's `typeName` once into a `Map` (O(n)) and
  passes a resolver reading the cache, so the O(n log n) comparisons no longer recompute `typeName` (and
  re-allocate its template string) per comparison. Other keys take the original path — zero overhead.

2026-07-18 — Tests: added a behaviour test in `sort.test.ts` (real `typeName`, asserts type order +
folders-first + name tiebreaker) and a new `sort.typecache.test.ts` that mocks `./filetypes` to count calls
and asserts `typeName` is invoked exactly `entries.length` times for a type sort and zero times for
name/size/modified. Hit a vitest gotcha: `beforeEach(() => spy.mockClear())` implicitly returns the spy,
which vitest registers as a teardown and calls with no args — fixed with a block-body hook.

2026-07-18 — `npm run check` clean (0/0). Full suite green: 696 tests / 74 files (+3).

## Resolution
A file list sorted by **Type** was calling `typeName()` on both operands of every comparison — O(n log n)
calls, each re-deriving the label and allocating a fresh `` `${EXT} File` `` string for uncatalogued
extensions, on the hot sort path CPE-688 targets. `sortEntries` now decorates once (O(n) `typeName` calls
cached in a `Map`) and compares cached values via a new backward-compatible `typeNameOf` resolver on
`compareEntries`; non-type keys are untouched. Order is byte-for-byte identical (verified by existing +
new tests). Headless and behaviour-preserving — safe to land unattended. Files: `src/lib/sort.ts`,
`src/lib/sort.test.ts`, `src/lib/sort.typecache.test.ts`. Advances epic CPE-688.
