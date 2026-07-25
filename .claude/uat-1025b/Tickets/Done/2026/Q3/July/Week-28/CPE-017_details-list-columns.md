---
id: CPE-017
title: Details view — sortable Name / Date modified / Type / Size columns
type: Feature
status: Done
priority: High
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Replace the flat icon+name+size row with Explorer's details view: a column header row with Name,
Date modified, Type and Size, each sortable, with a sort-direction indicator.

## Acceptance Criteria

- [x] Column header row with Name, Date modified, Type, Size
- [x] Clicking a header sorts by that column; clicking again reverses
- [x] Active sort column shows a direction chevron
- [x] Folders continue to sort before files
- [x] Date and type formatting are pure functions with unit tests

## Resolution

Replaced the flat row with Explorer's details view: a sticky column header row (Name, Date modified,
Type, Size) over a grid of rows sharing the same `grid-template-columns`, so headers and cells stay
aligned. Clicking a header sorts by it; clicking the active header flips direction; the active column
shows an up/down chevron. Folders always sort before files regardless of the active column, matching
Explorer.

Formatting is pure and unit-tested: `formatDate` (`src/lib/datetime.ts`, 6 tests) renders Explorer's
`M/D/YYYY h:mm AM/PM`. Its tests build timestamps in **local** time rather than hard-coding epoch
values, because CI runs in UTC and the dev machine doesn't — a naive test would pass locally and fail
in CI. Midnight-as-12AM and noon-as-12PM are covered explicitly; both are classic off-by-one bugs.

## Work Log

2026-07-11 — Picked up. Built the sticky header + grid rows sharing one column template so they stay aligned.
2026-07-11 — Wrote formatDate with tests built from LOCAL time — hard-coded epochs would pass locally and fail in UTC CI.
2026-07-11 — Covered midnight (12 AM) and noon (12 PM) explicitly. Closing as Done.

## Notes
