---
id: CPE-029
title: Multi-selection (Ctrl+click, Shift+click, Ctrl+A)
type: Feature
status: Done
priority: Critical
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Selection is a single index. Every file operation (copy, cut, delete, properties) acts on a
*selection*, so multi-select is the foundation the rest of the file-operation work stands on.

## Acceptance Criteria

- [x] Click selects one; Ctrl+click toggles an item in/out of the selection
- [x] Shift+click selects a contiguous range from the anchor
- [x] Ctrl+A selects all; Escape clears
- [x] Status bar shows "N items selected" and the combined size
- [x] Details pane shows a multi-selection summary rather than one file
- [x] Selection logic is a pure, unit-tested module

## Resolution

Wrote `src/lib/selection.ts` as a pure, immutable module (14 unit tests) before touching any UI:
plain click, Ctrl+click toggle, Shift+click range from the anchor, Ctrl+Shift+click to add a range,
select-all, Escape to clear, and arrow/Shift-arrow lead movement with clamping.

The subtle one is `remapByPath`: indices are meaningless across a re-sort, re-filter, or refresh, so
after any listing change the selection is re-derived from the selected **paths**, and anything that
vanished is dropped. Without this, sorting a folder would silently move your selection onto different
files — a genuinely dangerous bug once Delete is wired up.

Status bar shows the count and combined size; the details pane shows a multi-selection summary.

## Work Log

2026-07-11 — Built selection.ts as a pure module with 14 tests first; every file operation depends on it being right.
2026-07-11 — Added remapByPath: after a sort/filter/refresh, selection is re-derived from paths, not indices. Index-based selection would silently retarget Delete onto the wrong files.
2026-07-11 — Closing as Done.

## Notes
Blocks CPE-030..035.
