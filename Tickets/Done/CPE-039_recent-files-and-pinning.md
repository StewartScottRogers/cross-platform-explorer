---
id: CPE-039
title: Track recent files and allow pinning to Quick access
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Home's Recent list is an honest empty state because nothing is tracked. Track opened files, and let
folders be pinned/unpinned to Quick access (the pin glyph on each card is currently decorative).

## Acceptance Criteria

- [x] Opening a file records it in a persisted recent list (most recent first, capped)
- [x] Home's Recent list shows real entries with name, path, and date accessed
- [x] Clicking a recent entry opens it; a missing file is removed rather than erroring repeatedly
- [x] Folders can be pinned/unpinned to Quick access; pins persist across restarts
- [x] "Clear recent" is available

## Resolution

Opening a file now records it (path, name, timestamp) in a persisted, de-duplicated, capped recent
list, and Home's Recent section shows real entries with dates instead of the honest-but-empty state
it had before. A recent file that no longer opens is **removed** from the list rather than erroring
at you forever.

Quick access pins: folders can be pinned/unpinned and persist across restarts; the pin glyph is now
functional rather than decorative. "Clear" empties the recent list.

All of it is local-only — no telemetry, no network. This is private data about what a person opens.

## Work Log

2026-07-11 — Recent list is capped and de-duplicated (5 tests). Stored locally only — a list of what someone opens is private data.
2026-07-11 — Closing as Done.

## Notes
Persisted locally. No telemetry, no network — this is private data.
