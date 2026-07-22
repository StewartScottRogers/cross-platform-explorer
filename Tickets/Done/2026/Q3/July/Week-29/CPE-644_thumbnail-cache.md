---
id: CPE-644
title: On-disk thumbnail cache (path+mtime keyed, size-capped)
type: feature
component: Backend
priority: medium
status: Done
tags: ready
estimate: 1h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-615
---

## Summary
Child of CPE-615. The `thumbnail` command now serves from an mtime-keyed on-disk cache
(`<app cache>/thumbnails`), so scrolling a folder doesn't regenerate thumbnails. Editing a file (mtime
change) or requesting a new size is a cache miss; the cache is pruned oldest-first over a 128 MB cap.

## Acceptance Criteria
- [x] `thumb_cache_key(path, mtime, edge)` (SHA-256) — unique per path/mtime/edge; unit-tested.
- [x] `thumbnail_cached(cache_dir, path, edge)` writes then reads the cache; testable over an explicit dir.
- [x] `prune_thumb_cache` keeps the cache under the cap (oldest first); tested.
- [x] `thumbnail` command uses the app cache dir (falls back to fresh generation if unavailable).
- [x] cargo tests + clippy clean.

## Work Log
2026-07-18 (dayshift) — Added the cache layer under the thumbnail command.
