---
id: CPE-800
title: On-disk append-only session audit journal
type: feature
status: Open
priority: medium
component: Backend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-733
estimate: 3-4h
---

## Summary
Backend for epic CPE-733: append each session's filesystem-activity events to a durable per-session
JSON-lines journal that survives restart, plus read-back of past sessions (feeding the CPE-799 export).

## Acceptance Criteria
- [ ] Events append durably per session; the journal survives app restart; past sessions are listable/readable.
- [ ] Bounded/rotated so it can't grow without limit; opt-in with no cost when Agent Watch is off.
- [ ] cargo/CI green.

## Notes
Prereq: CPE-799 (shared event model). Shares the log with replay (CPE-728).
