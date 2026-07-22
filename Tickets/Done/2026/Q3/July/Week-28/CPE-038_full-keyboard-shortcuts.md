---
id: CPE-038
title: Complete the Explorer keyboard shortcut set
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Implement the standard Explorer shortcuts we are missing, so muscle memory transfers.

## Acceptance Criteria

- [x] Ctrl+T new tab, Ctrl+W close tab, Ctrl+Tab / Ctrl+Shift+Tab switch tabs
- [x] Ctrl+A select all, Ctrl+F focus search, F5 refresh, Alt+Up go up
- [x] Ctrl+Shift+N new folder, F2 rename, Del / Shift+Del delete
- [x] Ctrl+C/X/V clipboard, Alt+Enter properties, Alt+P toggle details pane
- [x] Shortcuts are ignored while an inline editor or text field has focus
- [x] A shortcut cheatsheet is documented in the README

## Resolution

Implemented the standard set: Ctrl+T/W/Tab/Shift+Tab (tabs), Ctrl+A, Ctrl+F, Ctrl+L, Alt+D, F5,
Alt+Up, Backspace, Ctrl+Shift+N, F2, Del, Shift+Del, Ctrl+C/X/V, Ctrl+Z, Alt+Enter, Alt+P,
Home/End, and Shift+arrows for range selection.

Every shortcut is suppressed while an INPUT/TEXTAREA has focus or an inline rename is active — so
typing "delete" into the search box cannot delete your files. That guard is the whole reason this
ticket is not just a switch statement.

## Work Log

2026-07-11 — The important part is the guard: shortcuts are inert while typing. Without it, typing in the search box could trigger Delete.
2026-07-11 — Closing as Done.

## Notes
