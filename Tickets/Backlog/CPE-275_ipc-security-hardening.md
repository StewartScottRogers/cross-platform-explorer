---
id: CPE-275
title: IPC security hardening (auth, no cross-sidecar visibility)
type: Task
status: Open
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
---

## Summary

Lock down the host↔sidecar channel. Each sidecar's channel is authenticated to that
process, capabilities are enforced server-side, and there is no discoverable path
for one sidecar to reach another or for an outside process to impersonate a
sidecar. Complements the secret-broker guarantees.

## Acceptance Criteria

- [ ] Per-sidecar channel with a launch-time token; unauthenticated/foreign
      connections rejected.
- [ ] Capability checks enforced at the broker, not trusted from the client side.
- [ ] No shared channel/registry that exposes one sidecar to another.
- [ ] Threat-model note in the ADR; tests for impersonation + undeclared-capability
      rejection.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-266]], [[CPE-265]]. **Phase:** P5. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
