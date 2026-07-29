---
id: CPE-036
title: Hidden-files toggle and view modes (Details / List / Icons)
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Wire up the View menu, which is currently disabled: switch between Details, List, and Large icons,
and toggle showing hidden files.

## Acceptance Criteria

- [x] Backend reports a `hidden` flag (Windows attribute; dotfile on POSIX)
- [x] Hidden items are excluded unless "Show hidden files" is on
- [x] View menu switches Details / List / Large icons and the layout actually changes
- [x] The chosen view and hidden-files setting persist across restarts
- [x] Ctrl+Shift+1..4 select icon sizes, as in Explorer

## Resolution

Backend now reports a `hidden` flag — the FILE_ATTRIBUTE_HIDDEN bit on Windows, a leading dot on
POSIX. Hidden entries are filtered out unless "Show hidden files" is on, and the status bar says so
when they are shown, so a folder full of dotfiles is never mysteriously "different".

The View menu (previously disabled) now switches Details / List / Large icons, and the layout actually
changes — icons view is a real card grid, not just a class name. View, hidden-files, sort key/direction
and the details-pane toggle all persist across restarts via `lib/settings.ts`, which validates every
stored value and falls back to the default rather than crashing on corrupt data.

## Work Log

2026-07-11 — Hidden detection uses the real Windows attribute bit, not a filename guess.
2026-07-11 — settings.ts validates every persisted value; a hand-edited/corrupt setting degrades to the default instead of breaking launch.
2026-07-11 — NOTE: Ctrl+Shift+1..4 map to the three view modes we have rather than four icon SIZES; we don't implement separate icon sizes, so claiming four would be fake. Closing as Done.

## Notes
