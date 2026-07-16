---
id: CPE-474
title: "Add resellers — DeepInfra, Novita AI, AI/ML API"
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
Add these resellers as descriptors (CPE-468) + unified manifests (CPE-471) + allow-listed egress
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: DeepInfra, Novita AI, AI/ML API (aimlapi).

Bases: api.deepinfra.com/v1/openai, api.novita.ai/v3/openai, api.aimlapi.com/v1.

## Acceptance Criteria
- [ ] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [ ] Each appears in the provider dropdown and launches an agent with its stored key.
- [ ] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [ ] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
OpenAI-compatible. Model manifests already exist (CPE-444) — wire as launch providers.
