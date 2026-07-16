---
id: CPE-478
title: "Add resellers — Requesty, Glama, Unify, Vercel AI Gateway, Portkey"
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
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Requesty, Glama, Unify, Vercel AI Gateway, Portkey.

Bases: router.requesty.ai/v1, glama.ai/api/gateway/openai/v1, api.unify.ai/v0, ai-gateway.vercel.sh/v1, api.portkey.ai/v1. Some need extra headers (descriptor `extra_headers`).

## Acceptance Criteria
- [ ] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [ ] Each appears in the provider dropdown and launches an agent with its stored key.
- [ ] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [ ] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
Multi-provider ROUTER gateways (OpenRouter-peers): one key, many upstream providers, OpenAI-compatible. Portkey/Vercel may need a gateway-config header.
