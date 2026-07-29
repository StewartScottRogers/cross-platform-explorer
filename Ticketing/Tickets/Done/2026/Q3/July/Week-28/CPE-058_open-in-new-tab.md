---
id: CPE-058
title: Open a folder in a new (background) tab from the context menu
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 45m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Tabs exist (Ctrl+T opens a Home tab) but there is no way to open a specific folder in a new tab. Add an
"Open in new tab" context-menu entry for a selected folder that creates a background tab seeded at that
folder's path, leaving the current tab active (Explorer/browser behaviour).

## Acceptance Criteria

- [ ] Right-clicking a folder shows "Open in new tab"; it is hidden/absent for files
- [ ] Choosing it adds a new tab whose title is the folder name, WITHOUT switching away from the current tab
- [ ] No-op for non-folders
- [ ] Integration test drives the context menu and asserts the new tab appears; check + suite green

## Resolution

Added `openInNewTab(entry)` (creates a background tab seeded at the folder path via `createHistory`,
no active-tab switch), an `"open-new-tab"` action case, and a folder-gated "Open in new tab" context-
menu entry driven by a new `folderSelected` prop. Integration test right-clicks the folder row, clicks
the menu item, and asserts a "notes" tab appears alongside the still-active "d" tab. `npm run check` 0
errors; suite 146 passed; `vite build` clean. Committed, merged to `main`, pushed.

## Work Log

2026-07-11 — Nightshift loop: tabs support a per-tab history; `newTab()` seeds Home. Reuse that to seed a folder path in the background. Tab titles derive from the path's last segment. Context menu gains a folder-only entry gated by a new `folderSelected` prop.

## Notes

Background (not foreground) matches "open in new tab" convention. Related: [[CPE-057]].
