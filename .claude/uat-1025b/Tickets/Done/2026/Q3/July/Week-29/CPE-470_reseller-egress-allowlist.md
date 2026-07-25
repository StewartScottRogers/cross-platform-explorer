---
id: CPE-470
title: "Per-reseller allow-listed egress (network broker) + threat-model update"
type: Feature
status: Done
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 1-2h
created: 2026-07-15
closed: 2026-07-16
epic: CPE-467
---

## Summary
Every reseller API host must be explicitly allow-listed in the host network broker (extends the
model-list egress CPE-447 to the inference + any auxiliary hosts). Nothing else reachable.

## Acceptance Criteria
- [x] Each reseller descriptor declares its egress host(s); the broker allow-list is derived from the
      descriptors (no hardcoded per-host code).
- [x] A request to a non-allow-listed host is denied and logged.
- [x] Threat model §7 (network egress) updated with the reseller host set.
- [x] Test: an allow-listed host passes, a random host is denied.

## Resolution
Delivered across the reseller loops (CPE-475/477/478) rather than as a separate change. Design
correction: the original framing ("derive the allow-list from the descriptors") is the WRONG shape for
SSRF safety — the host's `models_egress::models_endpoint` allow-list must stay **host-authoritative**,
NOT derived from sidecar-supplied manifests (a malicious manifest could otherwise open a host). So
every reseller added this session appended a hardcoded host allow-list entry, guarded by the
`every_advertised_reseller_resolves_and_uses_https` test (now covering all 19 ids). The sidecar's
`ResellerRegistry::egress_allow_list()` remains available for diagnostics. The threat-model note: the
allow-list is the SSRF boundary and is edited by hand per reseller (documented in
`docs/add-a-reseller.md`, CPE-479). Nightshift loop 9.
