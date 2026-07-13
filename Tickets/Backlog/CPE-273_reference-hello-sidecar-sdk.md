---
id: CPE-273
title: Reference "hello" sidecar + dev harness / SDK
type: Task
status: Open
priority: High
component: Multiple
estimate: 3-4h
created: 2026-07-13
---

## Summary

A minimal example sidecar that exercises the whole platform end-to-end (handshake,
each capability, a trivial UI surface) **without** the AI Console — proving the
pattern in isolation. Doubles as the reference/SDK others copy to build a new
sidecar, and as the fixture for supervisor/broker/UI tests.

## Acceptance Criteria

- [ ] `examples/hello-sidecar` implementing the contract: handshake, uses context,
      secrets, storage, and event capabilities, renders a tiny UI in the mount.
- [ ] A documented "build your own sidecar" starter (SDK helpers + template).
- [ ] Used as the test fixture by [[CPE-265]], [[CPE-266]], [[CPE-271]], [[CPE-272]].
- [ ] Ships only in dev/example builds, never in the shipped explorer.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-267]], [[CPE-268]], [[CPE-269]], [[CPE-270]]. **Phase:** P4.
**Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
