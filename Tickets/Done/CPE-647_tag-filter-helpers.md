---
id: CPE-647
title: Tag filter helpers (filter entries by tag)
type: feature
component: Frontend
priority: low
status: Done
tags: ready
estimate: 20m
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Child of CPE-614. Pure helpers for the "show only files tagged X" view: `filterEntriesByTag` and
`anyEntryHasTag`. The sidebar/context wiring (CPE-639) consumes these.

## Acceptance Criteria
- [x] `filterEntriesByTag(entries, store, tag)` (blank = all, unknown = none) + `anyEntryHasTag`; unit-tested.
- [x] `npm run check` clean; vitest green.

## Work Log
2026-07-18 (dayshift) — Built the pure filter core ahead of the sidebar wiring.
