---
id: CPE-036
title: Hidden-files toggle and view modes (Details / List / Icons)
type: Feature
status: Open
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Wire up the View menu, which is currently disabled: switch between Details, List, and Large icons,
and toggle showing hidden files.

## Acceptance Criteria

- [ ] Backend reports a `hidden` flag (Windows attribute; dotfile on POSIX)
- [ ] Hidden items are excluded unless "Show hidden files" is on
- [ ] View menu switches Details / List / Large icons and the layout actually changes
- [ ] The chosen view and hidden-files setting persist across restarts
- [ ] Ctrl+Shift+1..4 select icon sizes, as in Explorer

## Resolution
## Work Log
## Notes
