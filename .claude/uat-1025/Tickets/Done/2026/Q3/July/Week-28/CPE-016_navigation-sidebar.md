---
id: CPE-016
title: Navigation sidebar — Home, special folders, drives, expandable tree
type: Feature
status: Done
priority: High
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Left navigation pane: Home and Gallery entries, the user's special folders, drives, each expandable
with a chevron to reveal child folders, matching Explorer's tree.

## Acceptance Criteria

- [x] Sidebar lists Home, special folders (Desktop/Documents/Downloads/Pictures/Music/Videos), and drives
- [x] Chevrons expand/collapse to show child directories, loaded lazily
- [x] Clicking an item navigates the content pane
- [x] The active location is highlighted
- [x] Expansion state persists while the app is open

## Resolution

Sidebar lists Home, Gallery (disabled — not implemented), the user's special folders from
`special_folders`, and drives from `list_drives`. Each folder/drive has a chevron twisty that lazily
loads its child directories via `list_dir` on first expand, caching them so re-expanding is instant.
Only directories are shown in the tree (files would be noise). The active location is highlighted.
Expansion state persists for the session.

An unreadable folder resolves to an empty child list showing "No folders" — it does not spin forever
or throw, which matters because expanding a protected system folder is a normal thing to do by accident.

## Work Log

2026-07-11 — Picked up. Built the tree with lazy child loading + caching.
2026-07-11 — Handled the unreadable-folder case explicitly: empty child list + "No folders" rather than a hang or an unhandled rejection.
2026-07-11 — Closing as Done.

## Notes
