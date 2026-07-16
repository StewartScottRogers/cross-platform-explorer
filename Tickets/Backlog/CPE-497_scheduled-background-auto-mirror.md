---
id: CPE-497
title: "Scheduled / background auto-mirror with a pause control"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-16
epic: CPE-488
---

## Summary
Sync trigger (Q2): in addition to on-demand, auto-mirror on an **interval / on window focus**, with a
visible **pause/disable** control and status surfacing. Off by default; respects the per-repo policy;
never background-force-pushes.

## Acceptance Criteria
- [ ] Per-repo auto-sync toggle + interval (off by default).
- [ ] Auto-sync runs on the schedule / on focus, using the same safe planner as manual sync.
- [ ] A clear pause/disable control; last-sync time + any error surfaced.
- [ ] Never force-pushes in the background; a divergence pauses + surfaces rather than reconciling blindly.
- [ ] Tests for the scheduler + off-by-default behaviour.

## Notes
**needs-prereq:** [[CPE-495]] (reuses its sync actions + policy).
