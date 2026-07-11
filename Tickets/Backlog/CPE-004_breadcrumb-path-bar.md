---
id: CPE-004
title: Add a breadcrumb path bar with click-to-navigate
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-10
closed:
---

## Summary

The toolbar currently shows the raw current path as static text. Turn it into a breadcrumb of
clickable segments so the user can jump to any ancestor directory in one click.

## Acceptance Criteria

- [ ] The current path renders as clickable segments split on the path separator
- [ ] Clicking a segment navigates to that ancestor directory
- [ ] The active/last segment is styled distinctly and is not a link
- [ ] Works with both Windows (`\`) and POSIX (`/`) separators
- [ ] Long paths truncate gracefully without breaking the toolbar layout

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

*(Agent appends dated entries here throughout — do not fill in)*

## Notes

May want a small Rust `split_path` command, or handle splitting in the frontend using the separator
returned from the backend to stay cross-platform.
