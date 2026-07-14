---
id: CPE-367
title: "Threat model: cover host-mediated network egress (key verification)"
type: Task
status: Done
priority: Medium
component: Docs
created: 2026-07-14
closed: 2026-07-14
---

## Summary

CPE-347 added the platform's first outbound network path — the host verifies a provider API key
on the sidecar's behalf (`host.verify_key`). The security threat model
(`docs/security/threat-model.md`, authored under CPE-304 before CPE-347) predates this surface and
its §5 still asserts the UI/network posture without accounting for host egress. Keep the security
review honest with the code: add a STRIDE section for host-mediated network egress and record the
SSRF-containment invariant.

## Acceptance
- New threat-model section covering `host.verify_key` egress: SSRF containment (host-chosen,
  allow-listed URL — sidecar never supplies it), key-in-transit, MITM/forged-verdict handling,
  DoS timeout, privacy.
- Invariant table gains "no SSRF / arbitrary egress from a sidecar" → ✅.
- Scope + trust-boundary intro updated to include the provider-API egress boundary.
- Feeds CPE-304's still-open final sign-off (which remains gated on CPE-296 / CPE-322).

## Work Log
2026-07-14 — Filed as follow-on hygiene from CPE-347.
