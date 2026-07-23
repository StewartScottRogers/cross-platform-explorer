---
id: CPE-916
title: Activity timeline bucketing (scrubbable replay core)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-728
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
First headless slice of activity replay & scrub (CPE-728). `cpe_server::activity_timeline`:
- `bucketize(events, window_ms) -> Vec<TimelineBucket>` — folds recorded `AuditEvent`s into epoch-aligned
  time windows (count, per-kind counts, distinct sessions), sorted; only non-empty buckets. `window_ms` 0
  → 1 (no div-by-zero).
- `summarize(events) -> ActivitySummary` — totals, per-kind, distinct sessions, first→last span.

The scrubbable timeline / minimap UI renders these buckets; the scrubber steps through them.

## Acceptance Criteria
- [x] Events bucket into aligned windows with per-kind + per-session counts; empty stream → no buckets.
- [x] Summary totals/kinds/sessions/span (0 for empty). 3 unit tests; clippy `-D warnings` clean.

## Work Log
- 2026-07-22 — Activated CPE-728 with the timeline compute core over the existing audit-journal events. The
  scrub UI (timeline/minimap, playback, state-at-cursor reconstruction) is the GUI remainder.
