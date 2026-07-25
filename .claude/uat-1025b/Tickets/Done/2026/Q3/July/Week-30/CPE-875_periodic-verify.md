---
id: CPE-875
title: Periodic integrity re-verify while running (integrity monitor)
type: feature
component: Frontend
priority: low
status: Done
tags: ready
epic: CPE-737
created: 2026-07-21
closed: 2026-07-21
---

## Summary
Completes CPE-737's "periodically re-verify" DoD: when the opt-in monitor is on, re-check all baselined
folders every 6 hours while the app stays open (not only at startup), so a long-running session still
catches silent corruption without a restart. Reuses the tested verify + summary-notice path.

## Acceptance Criteria
- [x] When `verifyOnStartup` is on, a 6-hour interval re-runs `verifyAllBaselines`; cleared on teardown.
- [x] Still opt-in / off by default (same toggle); nothing runs in the background otherwise.
- [x] `npm run check` + suite (902) green.

## Work Log
- 2026-07-21 (autonomous) — setInterval in onMount (gated by the opt-in), cleared in onDestroy. Epic CPE-737
  "periodic monitoring" now fully satisfied.
