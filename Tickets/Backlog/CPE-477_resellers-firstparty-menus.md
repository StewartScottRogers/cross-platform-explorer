---
id: CPE-477
title: "Add resellers — Perplexity, Mistral, DeepSeek, Cohere"
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
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Perplexity (Sonar), Mistral La Plateforme, DeepSeek, Cohere.

Bases: api.perplexity.ai, api.mistral.ai/v1, api.deepseek.com, api.cohere.ai/compatibility/v1.

## Acceptance Criteria
- [ ] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [ ] Each appears in the provider dropdown and launches an agent with its stored key.
- [ ] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [ ] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
First-party APIs that expose a menu of their own models; OpenAI-compatible (Cohere via its compat endpoint).
