---
id: CPE-684
title: Async cancellable remote listing + transfer integration
type: feature
component: Multiple
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-616
estimate: 3-4h
---

## Summary
Child of CPE-616. Remote ops are slow + fail: make remote listing async + cancellable (reuse the
streaming-liveness pattern, CPE-662) with clear latency/error states, and route to/from-remote copies
through the transfer queue (CPE-613). Prereq: CPE-682/683.

## Acceptance Criteria
- [ ] Remote listing streams + is cancellable; latency/error states are clear.
- [ ] Copy to/from a remote runs through the transfer manager with progress.
- [ ] `npm run check` + suite green.

## Work Log
