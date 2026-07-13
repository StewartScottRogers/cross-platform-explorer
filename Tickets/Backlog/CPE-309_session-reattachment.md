---
id: CPE-309
title: Session reattachment across sidecar restart
type: Feature
status: Open
priority: Medium
component: Backend
estimate: 3-4h
created: 2026-07-13
---

## Summary

A long-running agent session shouldn't die because the AI Console sidecar was
restarted (crash, update, or user toggle). Decide and implement how running PTY
sessions survive: either the PTY-owning process outlives the sidecar UI process and
re-attaches, or sessions checkpoint and resume cleanly.

## Acceptance Criteria

- [ ] A running agent session survives a sidecar restart, or fails gracefully with
      the transcript preserved — never a silent kill of the user's work.
- [ ] Reattach restores live I/O to the console UI; state is reconciled ([[CPE-299]]).
- [ ] Interaction with resource budgets ([[CPE-297]]) and reaping is defined.
- [ ] Tested: restart the sidecar mid-session, assert the session is recoverable.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], [[CPE-265]]. **Phase:** C2/C3. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
