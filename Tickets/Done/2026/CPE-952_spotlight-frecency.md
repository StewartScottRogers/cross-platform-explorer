---
id: CPE-952
title: Spotlight frecency ranking (recent + frequent)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-704
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
Third headless slice of the spotlight overlay (CPE-704). `cpe_server::spotlight_frecency`:
- `Visit { path, count, last_used_s }` + `recency_weight(age)` (Firefox-style buckets: hour 4×, day 2×,
  week 1×, month 0.5×, older 0.25×) + `frecency(v, now) = count × recency_weight(age)`.
- `rank_frecent(visits, now, limit) -> Vec<path>` — the overlay's default (empty-query) view: recent-and-
  frequent items first, tie-broken by recency then path.

## Acceptance Criteria
- [x] Recency weight decays by bucket; frecency = count × weight; recent+frequent beats stale/rare.
- [x] Limit caps; empty safe. 4 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Third CPE-704 slice: frecency for the spotlight's no-query default list.
