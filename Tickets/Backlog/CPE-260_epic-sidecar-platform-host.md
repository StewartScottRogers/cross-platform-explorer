---
id: CPE-260
title: "EPIC: Sidecar platform (host)"
type: Task
status: Open
priority: High
component: Multiple
estimate: 4h+
created: 2026-07-13
---

## Summary

Build the reusable **host platform** that lets the explorer run any Mega-Feature
as an isolated sidecar process behind a single versioned contract. This epic
delivers the *pattern*, not any one feature — the AI Console ([[CPE-261]]) is its
first tenant, and future Mega-Features are added as further sidecars with **no
host code change**. Governed by [[CPE-259]].

## Child tickets

- [[CPE-262]] Contract/SDK crate: protocol & message envelope
- [[CPE-263]] Contract: version negotiation & semver policy
- [[CPE-264]] Sidecar manifest schema + registry (bundled + user dir)
- [[CPE-265]] Process supervisor (spawn / health / restart / shutdown)
- [[CPE-266]] Capability broker core (scoped grant/deny)
- [[CPE-267]] Capability: context provider (folder / repo / selection)
- [[CPE-268]] Capability: secrets broker (OS keychain, scoped)
- [[CPE-269]] Capability: storage namespace
- [[CPE-270]] Capability: event / notification bus
- [[CPE-271]] UI mount pane (host embeds a sidecar's own UI)
- [[CPE-272]] Isolation "delete-test" + CI guard
- [[CPE-273]] Reference "hello" sidecar + dev harness / SDK
- [[CPE-274]] Platform management UI (enable/disable, health/status)
- [[CPE-275]] IPC security hardening (auth, no cross-sidecar visibility)
- [[CPE-276]] Sidecar packaging & independent update

## Schedule (dependency-ordered waves)

- **P1 — Contract foundation:** CPE-262 → CPE-263, CPE-264
- **P2 — Runtime core:** CPE-265, CPE-266 (need P1)
- **P3 — Capabilities:** CPE-267, CPE-268, CPE-269, CPE-270 (need P2)
- **P4 — Surface + proof (Platform MVP):** CPE-271, CPE-273, then CPE-272
  (need P3). *Exit criterion: the hello sidecar runs isolated in-app and the
  delete-test is green in CI.*
- **P5 — Ops & hardening:** CPE-274, CPE-275, CPE-276 (need P4)

**Depends on:** [[CPE-259]]. **Blocks:** [[CPE-261]] (AI Console starts after P4).

## Acceptance Criteria

- [ ] All child tickets Done.
- [ ] A brand-new sidecar can be added by dropping in a binary + manifest, with no
      change to explorer or platform code (proven by the hello sidecar).
- [ ] Delete-test green; explorer builds/ships with zero sidecars.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
