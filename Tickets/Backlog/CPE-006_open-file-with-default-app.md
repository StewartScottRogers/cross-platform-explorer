---
id: CPE-006
title: Open files with the OS default application on double-click
type: Feature
status: Open
priority: Medium
component: Multiple
estimate: 1h
created: 2026-07-10
closed:
---

## Summary

Double-clicking a file currently does nothing (only folders navigate). Use the already-installed
`opener` plugin to open files with their default OS application.

## Acceptance Criteria

- [ ] Double-clicking a file opens it via the opener plugin
- [ ] Double-clicking a folder still navigates into it (unchanged)
- [ ] Errors (missing handler, permission) surface in the status bar rather than crashing
- [ ] The opener permission is present in capabilities/default.json (it already is)

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

*(Agent appends dated entries here throughout — do not fill in)*

## Notes

`@tauri-apps/plugin-opener` is already a dependency; `opener:default` is already granted.
