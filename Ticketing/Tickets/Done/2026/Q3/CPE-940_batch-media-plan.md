---
id: CPE-940
title: Batch media operation planner (collision-safe output paths)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-723
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
First headless slice of batch media operations (CPE-723). `cpe_server::batch_media`:
- `enum MediaOp { Resize, Convert, Rotate, Flip, Rename, StripMetadata }` + `BatchJob { ops,
  non_destructive }` (non-destructive is the default/safe mode).
- `validate(job)` — rejects no-ops, bad rotate angles (not 90/180/270), empty convert ext / rename
  template / zero resize.
- `plan(job, inputs) -> Vec<PlannedItem{input, output, summary}>` — the pure core: computes each output
  path (ops' effect on stem/extension + a derived suffix), keeps it **non-destructive** (never overwrites
  a source) and **collision-safe** (same-target outputs disambiguated `-2`, `-3`…), and summarises. The
  transform engine executes the plan.

## Acceptance Criteria
- [x] Ops compose into an output path + summary; convert changes ext; rename template `{stem}/{n}/{ext}`.
- [x] Non-destructive default keeps outputs off the sources + disambiguates collisions; overwrite mode
  opt-in. 7 unit tests; clippy `-D warnings` clean.

## Work Log
- 2026-07-23 (dayshift) — Activated CPE-723 with the collision-safe output-path planner. The actual
  image/media transforms, the live before/after preview, and the progress panel are the remaining children.
