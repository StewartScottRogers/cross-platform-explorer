---
id: CPE-031
title: Right-click context menu
type: Feature
status: Open
priority: High
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Windows 11 shows a context menu with Cut/Copy/Rename/Share/Delete as quick-action icons at the top,
then Open, then further commands. Right-clicking empty space offers New folder / Paste / Refresh.

## Acceptance Criteria

- [ ] Right-click on an item opens a context menu positioned at the cursor
- [ ] Item menu: Open, Cut, Copy, Rename, Delete, Properties
- [ ] Empty-space menu: New folder, Paste, Refresh, Sort
- [ ] Right-clicking an unselected item selects it first (Explorer's behaviour)
- [ ] Menu closes on Escape, outside click, or after an action
- [ ] Menu never renders off-screen
- [ ] Actions that are not implemented are disabled, not fake

## Resolution
## Work Log
## Notes
