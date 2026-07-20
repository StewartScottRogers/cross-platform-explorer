---
id: CPE-778
title: Pure binary/byte diff (first-difference + differing ranges)
type: feature
status: Open
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed:
epic: CPE-722
estimate: 1-2h
---

## Summary
Foundation for binary/hex compare in the compare studio (epic CPE-722). A pure module
(`src/lib/byteDiff.ts`): given two byte buffers, report whether they are equal, the first differing offset,
and the differing byte ranges — so the hex-compare view (CPE-779) highlights differences over
`read_file_range` (CPE-772) without re-implementing the scan.

## Scope
- `byteDiff(a: Uint8Array, b: Uint8Array): { equal: boolean; firstDiff: number | null; ranges: Array<{ start: number; len: number }>; lengthDiffers: boolean }`.
- A range is a maximal run of differing byte positions (using the shorter length; trailing extra bytes of
  the longer buffer are one final range and set `lengthDiffers`).
- Pure + total (empty buffers, equal buffers, different lengths).

## Acceptance Criteria
- [ ] Equal buffers → `equal: true, firstDiff: null, ranges: []`.
- [ ] Differing buffers → correct `firstDiff` and coalesced differing ranges; different lengths flagged with
      a trailing range for the extra bytes.
- [ ] Pure + dependency-free; unit tests cover equal / single-diff / multi-range / length-mismatch / empty.

## Notes
Independent of CPE-777. Consumed by the hex-compare view in CPE-779. Headless.
