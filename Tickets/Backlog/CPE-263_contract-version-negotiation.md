---
id: CPE-263
title: Contract version negotiation & semver policy
type: Task
status: Open
priority: High
component: Backend
estimate: 1-2h
created: 2026-07-13
---

## Summary

The contract is the only thing that can ricochet, so it must evolve safely. Define
the semver policy for `sidecar-contract` and implement handshake-time negotiation:
a sidecar built against contract vN works with a host advertising a compatible
range; incompatible versions fail cleanly with a clear message, never a crash.

## Acceptance Criteria

- [ ] Documented semver rules (additive = minor, breaking = major; host supports a
      range).
- [ ] Handshake negotiates the highest mutually-supported version.
- [ ] Incompatible sidecar → host refuses to mount it with an actionable error;
      other sidecars unaffected.
- [ ] Tests: compatible, forward-compatible-minor, and incompatible-major cases.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-262]]. **Phase:** P1. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
