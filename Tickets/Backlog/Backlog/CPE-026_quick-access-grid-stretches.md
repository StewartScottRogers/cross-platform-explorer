---
id: CPE-026
title: Quick access cards stretch into one long row on wide windows
type: Defect
status: Open
priority: Low
component: Frontend
estimate: 15m
created: 2026-07-11
closed:
---

## Summary

Maximized, the Quick access grid (`repeat(auto-fill, minmax(260px, 1fr))`) expands to 7 columns in a
single stretched row. Explorer keeps the cards in a compact 2–3 column block regardless of window
width.

## Acceptance Criteria

- [ ] Quick access grid is capped so it does not stretch across a maximized window
- [ ] Still reflows down to one column on a narrow window
- [ ] Cards remain left-aligned, matching Explorer

## Resolution
## Work Log
## Notes
