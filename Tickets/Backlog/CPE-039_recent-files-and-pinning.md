---
id: CPE-039
title: Track recent files and allow pinning to Quick access
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Home's Recent list is an honest empty state because nothing is tracked. Track opened files, and let
folders be pinned/unpinned to Quick access (the pin glyph on each card is currently decorative).

## Acceptance Criteria

- [ ] Opening a file records it in a persisted recent list (most recent first, capped)
- [ ] Home's Recent list shows real entries with name, path, and date accessed
- [ ] Clicking a recent entry opens it; a missing file is removed rather than erroring repeatedly
- [ ] Folders can be pinned/unpinned to Quick access; pins persist across restarts
- [ ] "Clear recent" is available

## Resolution
## Work Log
## Notes
Persisted locally. No telemetry, no network — this is private data.
