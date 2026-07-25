---
id: CPE-026
title: Quick access cards stretch into one long row on wide windows
type: Defect
status: Done
priority: Low
component: Frontend
estimate: 15m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Maximized, the Quick access grid (`repeat(auto-fill, minmax(260px, 1fr))`) expands to 7 columns in a
single stretched row. Explorer keeps the cards in a compact 2–3 column block regardless of window
width.

## Acceptance Criteria

- [x] Quick access grid is capped so it does not stretch across a maximized window
- [x] Still reflows down to one column on a narrow window
- [x] Cards remain left-aligned, matching Explorer

## Resolution

Capped `.qa-grid` at `max-width: 860px`. It still uses `repeat(auto-fill, minmax(260px, 1fr))`, so it
reflows to a single column when narrow, but no longer stretches into one long row of 7 cards across a
maximized window.

## Work Log

2026-07-11 — Spotted by maximizing the real installed app, not by reading the CSS.
2026-07-11 — Capped the grid width. Closing as Done.

## Notes
