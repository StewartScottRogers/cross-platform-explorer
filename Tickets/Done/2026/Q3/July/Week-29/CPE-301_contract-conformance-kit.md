---
id: CPE-301
title: Contract conformance test kit
type: Test
status: Done
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

"Add more sidecars easily" only holds if a new sidecar can *prove* it implements
the contract. Ship a conformance kit: a reusable test suite (and a mock host) that
drives a candidate sidecar through handshake, version negotiation, every capability,
error cases, and shutdown, reporting pass/fail. Every tenant epic runs it in CI.

## Acceptance Criteria

- [ ] A mock host + test battery covering the full contract surface and edge/error
      paths.
- [ ] A sidecar author runs one command to get a compliance report.
- [ ] The hello sidecar ([[CPE-273]]) and the AI Console ([[CPE-277]]) both pass it
      in CI.
- [ ] Versioned alongside the contract; new contract features add conformance cases.

## Resolution

Implemented `conformance` in `sidecar-host`: a transport-agnostic battery
(`run_conformance(channel, host_version) -> Report`) driving a sidecar through the
protocol — well-formed Hello, envelope schema version, version negotiability, reaching
Ready after Welcome, id-correlated responses, and an error (not silence) for an
unknown method. Talks over the `SidecarChannel` trait so it runs against an in-memory
mock now and a real child process later (wired by the supervisor [[CPE-265]]). Proven
by 4 tests: a well-behaved mock passes all 6 checks; injected faults (wrong
correlation, missing Ready, major-incompatible) are each caught.

**Grows with the contract** (as the ticket notes); the [[CPE-273]] hello sidecar and
[[CPE-277]] AI Console will run it in CI once those processes exist.

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — Implemented the kit + mock-validated battery (4 tests) during dayshift.
Done.
