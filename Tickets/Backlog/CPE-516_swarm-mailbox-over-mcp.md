---
id: CPE-516
title: "Swarm — inter-agent mailbox over the MCP layer"
type: Feature
status: Open
priority: Medium
component: Sidecar
tags: [needs-prereq]
estimate: 3-4h
created: 2026-07-16
epic: CPE-502
sprint: SPR-01
---

## Summary
The **coordination channel** for Swarm ([[CPE-502]], wave 1): agents message each other over the
**existing MCP layer** ([[CPE-288]]/[[CPE-307]]) — one substrate they already speak, and a natural tie
to the shared-memory epic ([[CPE-504]]). Addressed messages (to a role/agent) + a broadcast channel.

## Acceptance Criteria
- [ ] An agent can **post** a message (addressed to a role/agent, or broadcast) and **read** its inbox
      over the MCP layer.
- [ ] Message envelope is defined (from, to, kind, body, timestamp); ordering per recipient preserved.
- [ ] Delivery is contained to the swarm (no external egress); redaction applies as elsewhere.
- [ ] Degrades safely if MCP is unavailable (clear error, no crash).
- [ ] Tests for the envelope + addressing/broadcast + inbox read.

## Notes
Wave 1 of [[CPE-502]]; MCP-transport per the activation decision. **needs-prereq:** the MCP plumbing
(CPE-288/307). Feeds the coordinator (CPE-517) and relates to [[CPE-504]].
