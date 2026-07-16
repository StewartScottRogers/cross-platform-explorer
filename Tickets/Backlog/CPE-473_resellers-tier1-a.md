---
id: CPE-473
title: "Add resellers — Together AI, Fireworks AI, Groq"
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
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Together AI, Fireworks AI, Groq.

Bases: together.xyz/v1, api.fireworks.ai/inference/v1, api.groq.com/openai/v1. Groq is fast but a narrower model menu; Together/Fireworks host large OSS menus.

## Acceptance Criteria
- [ ] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [ ] Each appears in the provider dropdown and launches an agent with its stored key.
- [ ] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [ ] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
All OpenAI-compatible (`/v1`). Model manifests already exist for these three (CPE-444) — this wires them as launch providers.
