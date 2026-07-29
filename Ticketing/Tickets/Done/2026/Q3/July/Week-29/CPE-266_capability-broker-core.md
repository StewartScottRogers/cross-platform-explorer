---
id: CPE-266
title: Capability broker core (scoped grant/deny)
type: Task
status: Done
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Implemented `broker` in `sidecar-host`: `decide_grants` computes the granted set as
`requested ∩ consented ∩ policy` (least privilege; consent comes from [[CPE-296]]),
and `Broker` records per-sidecar grants, registers pluggable `CapabilityProvider`s,
and `dispatch()` enforces the grant before routing a `Request` to the provider —
returning `CapabilityDenied` for ungranted calls, `Internal` for unknown methods or a
missing provider, never panicking. `capability_for_method` maps `"secrets.get"` →
`Secrets` etc. Providers (CPE-267–270) plug in later. Added `Ord` to
`contract::Capability` to key the broker's ordered maps. 7 broker tests; contract (7)
+ host (19) + clippy all green.

**Deferred to [[CPE-265]]:** re-checking the grant *at handshake* needs the live
handshake, which the supervisor owns; the enforcement primitive is complete here.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented broker core + tests during dayshift. Done (handshake
re-check integration lands with the supervisor CPE-265).
