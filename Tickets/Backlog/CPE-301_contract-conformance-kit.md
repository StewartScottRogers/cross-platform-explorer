---
id: CPE-301
title: Contract conformance test kit
type: Test
status: Open
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
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

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-262]], [[CPE-263]]. **Phase:** P1→P4 (grows with the contract).
**Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
