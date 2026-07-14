---
id: CPE-260
title: "EPIC: Sidecar platform (host)"
type: Task
status: Open
priority: High
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-13
---

## Summary

Build the reusable **host platform** that runs any Mega-Feature as an isolated
sidecar process behind one small, versioned contract. This epic delivers the
*pattern*, not any one feature — the AI Console ([[CPE-261]]) is its first tenant,
and future Mega-Features are added as further sidecars with **no host code change**.
Governed by the ADR [[CPE-259]].

## Child tickets

**Core**
- [[CPE-262]] Contract/SDK crate — protocol & message envelope
- [[CPE-263]] Contract version negotiation & semver policy
- [[CPE-264]] Sidecar manifest schema + registry
- [[CPE-265]] Process supervisor (spawn/health/restart/shutdown)
- [[CPE-266]] Capability broker core (scoped grant/deny)
- [[CPE-267]] Capability: context provider
- [[CPE-268]] Capability: secrets broker (OS keychain)
- [[CPE-269]] Capability: storage namespace
- [[CPE-270]] Capability: event/notification bus
- [[CPE-271]] UI mount pane
- [[CPE-273]] Reference "hello" sidecar + SDK
- [[CPE-274]] Platform management UI
- [[CPE-275]] IPC security hardening
- [[CPE-276]] Sidecar packaging, signing & independent update/rollback

**Cross-cutting (added in end-to-end hardening)**
- [[CPE-294]] Phase-0 technical de-risking spike
- [[CPE-295]] Manifest trust, provenance & signing (supply-chain) — **Critical**
- [[CPE-296]] Capability consent & permission UX
- [[CPE-297]] Resource governance & performance budgets
- [[CPE-298]] Observability: logging, tracing & diagnostics export
- [[CPE-299]] Error model & user-facing failure handling
- [[CPE-300]] Schema versioning & migration
- [[CPE-301]] Contract conformance test kit
- [[CPE-302]] Platform integration / E2E test harness
- [[CPE-303]] Sidecar scaffolder
- [[CPE-304]] End-to-end threat model & security review — **Critical**
- [[CPE-314]] Accessibility & i18n (shared with [[CPE-261]])

## Schedule (dependency-ordered waves)

- **P0 — De-risk:** [[CPE-294]]. *Gate: transport/PTY/UI-embed/keychain proven.*
- **P1 — Contract foundation:** [[CPE-262]] → [[CPE-263]], [[CPE-264]], [[CPE-300]],
  [[CPE-301]] (started).
- **P2 — Runtime core:** [[CPE-265]], [[CPE-266]], [[CPE-297]], [[CPE-298]], [[CPE-299]].
- **P3 — Capabilities & consent:** [[CPE-267]], [[CPE-268]], [[CPE-269]], [[CPE-270]],
  [[CPE-296]], [[CPE-295]].
- **P4 — Surface + proof (Platform MVP):** [[CPE-271]], [[CPE-273]], [[CPE-302]],
  then [[CPE-272]]. *Exit: hello sidecar runs isolated, conformance + E2E + delete-test
  green in CI.*
- **P5 — Ops & hardening:** [[CPE-274]], [[CPE-275]], [[CPE-276]], [[CPE-303]],
  [[CPE-314]], then [[CPE-304]] sign-off. *Exit: security review passed.*

**Depends on:** [[CPE-259]]. **Blocks:** [[CPE-261]] (starts after P4).

## Definition of Done (epic-level gates)

- [ ] All child tickets Done.
- [ ] **Delete-test** green: explorer builds/ships/runs with zero sidecars; removing
      one sidecar affects nothing else ([[CPE-272]]).
- [ ] A brand-new sidecar can be added by manifest + binary with **no** host/platform
      code change (proven by the hello sidecar + scaffolder).
- [ ] Conformance kit + E2E harness pass in CI on all three OSes.
- [ ] Security review ([[CPE-304]]) signed off; no plaintext secrets, no unconsented
      code execution, no cross-sidecar reach, no UI escape.
- [ ] Performance budget met: zero startup delta when disabled; per-sidecar memory
      ceiling enforced.

## Key risks

- **Contract churn** — mitigated by P0 spike + conformance kit + semver.
- **Supply-chain RCE via manifests** — mitigated by [[CPE-295]]/[[CPE-296]]/[[CPE-304]].
- **Cross-platform PTY/keychain/webview quirks** — surfaced by [[CPE-294]] first.
- **Scope creep making the explorer heavy** — guarded by the delete-test + budgets.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Hardened: added P0 spike, security/trust/consent, resource budgets,
observability, error model, schema migration, conformance + E2E testing, scaffolder,
threat-model milestone, a11y. Reworked waves and added epic-level DoD + risks.
