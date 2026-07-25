---
id: CPE-944
title: Snapshot retention / thinning (grandfather-father-son)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-735
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
First headless slice of local snapshots / time-machine-lite (CPE-735). `cpe_server::snapshot_retention`:
- `Snapshot { id, epoch_s }` + `RetentionPolicy { hourly, daily, weekly, monthly }` (a sensible default of
  24/7/4/12).
- `thin(snapshots, policy) -> RetentionResult { keep, prune }` — grandfather-father-son thinning: for each
  tier (walking newest→oldest) keep the newest snapshot in each of the most-recent N distinct time buckets;
  a snapshot kept by any tier is kept, the rest pruned. Deterministic; keep/prune partition every input.

Pure; the snapshot engine takes + deletes the actual snapshots.

## Acceptance Criteria
- [x] Keeps newest-per-bucket up to the tier count; older tiers rescue snapshots the finer tiers dropped.
- [x] keep ∪ prune = all inputs, disjoint. 5 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Activated CPE-735 with the GFS retention policy. The snapshot capture engine
  (content-addressed, deduped) and the timeline/restore UI are the remaining children.
