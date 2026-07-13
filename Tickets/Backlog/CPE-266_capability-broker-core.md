---
id: CPE-266
title: Capability broker core (scoped grant/deny)
type: Task
status: Open
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
---

## Summary

Sidecars get no raw access to anything — they **request capabilities** at handshake
and the host grants scoped ones. This ticket builds the broker: the permission
model, the grant/deny decision, and the routing of capability calls to their
providers ([[CPE-267]]–[[CPE-270]]). Conceptually mirrors Tauri's capability model.

## Acceptance Criteria

- [ ] Capability request set declared in the manifest and re-checked at handshake.
- [ ] Broker grants only declared+allowed capabilities; ungranted calls are
      rejected with a typed error.
- [ ] Grants are per-sidecar and scoped (no ambient authority, no cross-sidecar).
- [ ] Pluggable provider registration so new capability types slot in.
- [ ] Tests: granted call succeeds, undeclared call denied, scope enforced.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-262]], [[CPE-265]]. **Phase:** P2. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
