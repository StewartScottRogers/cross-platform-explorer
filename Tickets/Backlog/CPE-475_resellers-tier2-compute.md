---
id: CPE-475
title: "Add resellers — Cerebras, SambaNova, Nebius, Hyperbolic"
type: Feature
status: Open
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-467
---

## Summary
Add these resellers as descriptors (CPE-468) + unified manifests (CPE-471) + allow-listed egress
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Cerebras Inference, SambaNova Cloud, Nebius AI Studio, Hyperbolic.

Bases: api.cerebras.ai/v1, api.sambanova.ai/v1, api.studio.nebius.ai/v1, api.hyperbolic.xyz/v1. Cerebras/SambaNova are ultra-low-latency; Nebius/Hyperbolic host broad OSS menus.

## Acceptance Criteria
- [ ] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [ ] Each appears in the provider dropdown and launches an agent with its stored key.
- [ ] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [ ] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
OpenAI-compatible compute-house inference APIs hosting many OSS models; new manifests needed.
