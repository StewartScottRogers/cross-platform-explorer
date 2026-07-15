---
id: CPE-440
title: "Forge threat-model section + offline/proxy"
type: Task
status: Open
priority: High
component: Multiple
tags: [needs-prereq]
estimate: 1-2h
created: 2026-07-15
epic: CPE-429
---

## Summary
Security review for the forge sidecar (CPE-429), extending CPE-304: token safety, per-provider
allow-listed egress, untrusted-clone-to-disk consent, and enterprise proxy/offline (reuse CPE-310).

## Acceptance Criteria
- [ ] Threat-model section: egress (one row per provider host), token in transit/at rest/logs,
      clone/pull brings untrusted content (consent/scan).
- [ ] Offline + corporate-proxy honoured (reuse CPE-369/310).
- [ ] Recorded in ADR 0001 once the vertical slice is verifiable.
