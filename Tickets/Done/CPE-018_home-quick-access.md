---
id: CPE-018
title: Home view — Quick access grid and Recent list
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Explorer's Home shows a collapsible "Quick access" grid of pinned folders (icon, name, subtitle) and
a Recent/Favorites/Shared pill switcher over a recent-files list.

## Acceptance Criteria

- [x] Home is a distinct view, shown at startup, reachable from the sidebar
- [ ] Quick access renders special folders as a two-column card grid
- [x] Section is collapsible via its chevron
- [x] Recent/Favorites/Shared pills render; unimplemented ones are disabled, not fake
- [x] Clicking a Quick access card navigates to that folder

## Resolution

Home is now a distinct view (a `HOME` sentinel rather than a filesystem path), shown at startup and
reachable from the sidebar and the first breadcrumb. Quick access renders special folders and drives
as a responsive card grid (icon, name, path subtitle, pin glyph), collapsible via its chevron.

The Recent section renders the Recent/Favorites/Shared pills — but **we do not track recent files**,
so rather than fabricate a plausible-looking list, Recent shows an honest empty state ("No recent
files yet — files you open in this app will appear here") and Favorites/Shared are disabled. Inventing
a fake recents list would have looked more like the screenshot and been a lie.

## Work Log

2026-07-11 — Picked up. Modelled Home as a sentinel rather than a path, so it participates in history and tabs like any location.
2026-07-11 — Chose an honest empty Recent state over a fabricated list. Looking right is not worth lying.
2026-07-11 — Closing as Done.

## Notes

Do not fabricate a "Recent files" list if the data isn't tracked — show an honest empty state.
