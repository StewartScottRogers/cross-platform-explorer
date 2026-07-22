---
id: CPE-612
title: Sort-by-date falls back to natural name order on ties
type: enhancement
component: Frontend
priority: low
status: Done
tags: ready
estimate: 15m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
The `type` and `size` sort keys break ties with natural name order, but `modified` did not — so files
sharing a timestamp (copied or extracted together, a common case) appeared in arbitrary backend order
when sorting by Date modified. Add the same name tiebreaker for consistency and determinism.

## Acceptance Criteria
- [x] `compareEntries(..., "modified", ...)` falls back to `compareNames` on equal timestamps.
- [x] A test asserts same-timestamp files order file1 < file2 < file10 (natural, not backend order).
- [x] `npm run check` clean; sort suite green.

## Resolution
`src/lib/sort.ts`: `modified` case now `((a.modified ?? 0) - (b.modified ?? 0)) || compareNames(...)`,
matching the type/size keys. Added the CPE-612 test.

## Work Log
2026-07-18 (Nightshift Loop 5) — Found while auditing the sort comparator for correctness; the
comparator was otherwise sound (transitive, numeric-aware). Minor consistency fix.
