---
id: CPE-432
title: "Repos sidecar process skeleton (contract handshake)"
type: Feature
status: Open
priority: High
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-429
---

## Summary
Stand up repos as a real sidecar tenant (CPE-429/260): handshake + capability request + protocol loop,
and its own loopback UI server, like the AI Console skeleton (CPE-277/271).

## Acceptance Criteria
- [ ] Emits Hello, reaches Ready, requests only needed capabilities (secrets, network-broker).
- [ ] Serves its own UI on loopback; announces the URL to the host.
- [ ] Bundled + wired behind the sidecar-platform feature; conformance kit passes.
- [ ] One-way dependency (only sidecar-contract); process isolation preserved.
