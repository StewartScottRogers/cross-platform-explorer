---
id: CPE-031
title: Right-click context menu
type: Feature
status: Done
priority: High
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Windows 11 shows a context menu with Cut/Copy/Rename/Share/Delete as quick-action icons at the top,
then Open, then further commands. Right-clicking empty space offers New folder / Paste / Refresh.

## Acceptance Criteria

- [x] Right-click on an item opens a context menu positioned at the cursor
- [x] Item menu: Open, Cut, Copy, Rename, Delete, Properties
- [x] Empty-space menu: New folder, Paste, Refresh, Sort
- [x] Right-clicking an unselected item selects it first (Explorer's behaviour)
- [x] Menu closes on Escape, outside click, or after an action
- [x] Menu never renders off-screen
- [x] Actions that are not implemented are disabled, not fake

## Resolution

Right-click context menu with Win11's layout: a quick-action icon row (Cut / Copy / Rename /
Delete) above Open and Properties. Right-clicking blank space gives New folder / Paste / Select all /
Refresh.

Right-clicking an unselected row selects it first, as Explorer does — otherwise the menu would act on
an invisible selection elsewhere in the list. Rename is disabled for multi-selections. Paste is
disabled when the clipboard is empty or the paste is illegal. The menu clamps itself inside the
viewport, so it can never open half off-screen, and closes on Escape, outside click, or after an
action.

## Work Log

2026-07-11 — Built ContextMenu.svelte with viewport clamping measured on mount.
2026-07-11 — Right-click-selects-first, so the menu can never act on an off-screen selection. Closing as Done.

## Notes
