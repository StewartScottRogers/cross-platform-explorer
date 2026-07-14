---
id: CPE-350
title: "Details view: drag to resize file-list columns"
type: Bug
status: Open
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

In the center pane's **Details** view, the column headers (Name / Date modified / Type / Size)
can't be resized by dragging their edges. The layout uses a fixed `--filelist-cols` grid
template with no drag handles. Add draggable dividers between the column headers that resize
the column to their left, mirroring the existing panel-divider pattern (CPE-069); persist the
widths.

## Design (frontend-only)
- `columns.ts`: pure `columnsTemplate(widths)` → grid template string, and
  `resizeColumnTo(widths, i, px, mins)` (clamped). Unit-tested.
- `FileList.svelte`: per-header resize handles; pointer-drag updates the widths and the
  `--filelist-cols` variable (header + rows share it, staying aligned); dispatch on drag end.
- `settings.ts`: persist `cpe.columnWidths`. `App.svelte`: load, pass, save (like sidebar/
  preview widths).

## Acceptance
- Dragging a column edge resizes that column; header and rows stay aligned; widths persist
  across restart; a double-click or min-clamp prevents collapsing to zero.
- `npm run check` + `npm test` green.

## Work Log
2026-07-13 — Reported by the user: center-pane columns can't be drag-resized. Filing + fixing.
