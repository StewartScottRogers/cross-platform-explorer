---
id: CPE-019
title: Details pane — selection preview on the right
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Right-hand pane showing a large icon and metadata for the current selection, or a summary of the
current folder when nothing is selected ("Home (42 items)" in the reference).

## Acceptance Criteria

- [x] Shows folder name + item count when nothing is selected
- [x] Shows name, type, size, modified date for a selected entry
- [x] Large icon reflects the entry type
- [x] Hidden when the Details toggle is off
- [x] Informational hint text when no selection, matching Explorer's tone

## Resolution

Right pane shows a 72px category icon plus metadata. With nothing selected it mirrors the reference
screenshot's "Home (42 items)" summary and the "Select a single file to get more information" hint.
With a row selected it shows name, Type, Size (files only — a folder's byte size is meaningless here),
Date modified, and full path. Hidden entirely when the Details toggle is off, and the main grid
reclaims the space.

## Work Log

2026-07-11 — Picked up. Built the pane to mirror the reference's empty and selected states.
2026-07-11 — Suppressed Size for folders — reporting 0 B for a directory is misleading. Closing as Done.

## Notes
