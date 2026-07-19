---
id: CPE-691
title: Perf benchmark harness + regression budget
type: test
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-688
estimate: 2-3h
---

## Summary
Child of CPE-688. A repeatable measure of time-to-first-paint and time-to-settled for folders of
~100/1k/10k/50k entries, split backend-walk vs frontend-render (console marks or a dev overlay), so the
"10×" is a falsifiable before/after. Add a smoke/budget test that fails if the file list regresses to
full-list rendering.

## Acceptance Criteria
- [ ] Reproducible timing for the reference folder sizes (paint + settle).
- [ ] A regression guard (test/budget) against full-list rendering.
- [ ] `npm run check` + suite green.

## Work Log
