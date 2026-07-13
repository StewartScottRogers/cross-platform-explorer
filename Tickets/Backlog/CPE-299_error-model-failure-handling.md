---
id: CPE-299
title: Error model & user-facing failure handling
type: Task
status: Open
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-13
---

## Summary

Define one error taxonomy for the contract so failures propagate predictably from
sidecar → host → user, each with a cause, whether it's retryable, and an actionable
message. Prevents the "silent failure" and "raw stack trace in a toast" outcomes as
the surface area explodes.

## Acceptance Criteria

- [ ] Typed error categories in the contract (handshake/version, capability-denied,
      transport, sidecar-crash, tool/agent failure, network, user-cancel).
- [ ] Each carries a stable code + human message + retryable flag.
- [ ] Host maps them to consistent UI (toast/inline/blocking) with a recovery hint;
      never a bare panic to the user.
- [ ] A sidecar crash mid-operation leaves reconcilable state, not corruption.
- [ ] Tests cover each category's propagation + presentation.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-262]], [[CPE-265]]. **Phase:** P2. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
