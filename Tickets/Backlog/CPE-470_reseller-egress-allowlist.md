---
id: CPE-470
title: "Per-reseller allow-listed egress (network broker) + threat-model update"
type: Feature
status: Open
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 1-2h
created: 2026-07-15
epic: CPE-467
---

## Summary
Every reseller API host must be explicitly allow-listed in the host network broker (extends the
model-list egress CPE-447 to the inference + any auxiliary hosts). Nothing else reachable.

## Acceptance Criteria
- [ ] Each reseller descriptor declares its egress host(s); the broker allow-list is derived from the
      descriptors (no hardcoded per-host code).
- [ ] A request to a non-allow-listed host is denied and logged.
- [ ] Threat model §7 (network egress) updated with the reseller host set.
- [ ] Test: an allow-listed host passes, a random host is denied.
