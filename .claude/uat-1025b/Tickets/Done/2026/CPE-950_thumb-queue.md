---
id: CPE-950
title: Thumbnail request priority queue (visible-first)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-718
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
Second headless slice of the thumbnail pipeline (CPE-718), pairing with `thumb_cache` (CPE-939).
`cpe_server::thumb_queue`:
- `Priority { Visible, Prefetch, Background }` + `ThumbQueue` with `enqueue(key, priority)` (dedupe;
  **promote** a re-requested key to a higher lane, never demote), `next()` (highest-priority first, FIFO
  within a lane), `contains`/`len`.

So on-screen thumbnails render before prefetch before background work. Pure scheduling; no image work.

## Acceptance Criteria
- [x] Dequeues Visible→Prefetch→Background, FIFO within a lane; enqueue dedupes.
- [x] Re-request promotes (scrolled on-screen) but never demotes. 4 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Second CPE-718 slice: the visible-first priority scheduler for thumbnail work.
