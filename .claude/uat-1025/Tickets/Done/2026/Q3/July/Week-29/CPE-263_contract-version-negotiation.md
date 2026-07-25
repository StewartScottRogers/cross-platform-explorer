---
id: CPE-263
title: Contract version negotiation & semver policy
type: Task
status: Done
priority: High
component: Backend
estimate: 1-2h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Primitive + policy in the contract crate ([[CPE-262]]); live-handshake enforcement in
`supervisor::handshake` ([[CPE-265]]) — on a major mismatch the host sends `Rejected`
and refuses to mount, others unaffected. Covered by unit tests and the real-process
E2E.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — **Partial:** the negotiation primitive is implemented and tested in the
contract crate ([[CPE-262]]) — `ContractVersion::is_compatible_with`, `negotiate()`
(same-major, lower-minor), and `VersionError::MajorMismatch`, with compatible /
forward-compatible-minor / incompatible-major tests all green. Documented semver
policy in ADR 0001. **Remaining:** wiring negotiation into the live handshake so the
host *refuses to mount* an incompatible sidecar with an actionable error while other
sidecars are unaffected — that enforcement belongs to the supervisor [[CPE-265]].
Returned to Backlog to finish alongside CPE-265.
2026-07-13 — Enforcement delivered in CPE-265 (`supervisor::handshake` sends Rejected
on major mismatch; unit + real-process E2E green). All criteria met. Done.
