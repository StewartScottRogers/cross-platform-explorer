---
id: CPE-256
title: Invert selection
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-13
---

## Summary

A file manager staple that's missing: **Invert selection** (Explorer's Home-tab
action). Select everything currently unselected in the visible list — handy for
"delete everything except these". Pure selection logic, fully unit-testable.

## Acceptance Criteria

- [ ] `invertSelection(sel, count)` in selection.ts flips selected/unselected
      across the visible rows, with unit tests.
- [ ] Empty-area context menu offers **Invert selection** next to Select all.
- [ ] `npm run check` and the full vitest suite pass.

## Resolution

Added pure `selectIndices(indices)` and `invertSelection(sel, count)` to
selection.ts (unit-tested) and an **Invert selection** row in the empty-area
context menu, wired to the `invert-selection` action. Frontend-only.

## Work Log
2026-07-13 — Filed and picked up during Nightshift.
2026-07-13 — Implemented + wired. Verified: vitest 254 pass (5 new selection
tests), npm run check clean. Landed on branch cpe-256-invert-selection.
