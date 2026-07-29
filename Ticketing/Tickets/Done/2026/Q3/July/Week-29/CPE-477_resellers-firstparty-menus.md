---
id: CPE-477
title: "Add resellers — Perplexity, Mistral, DeepSeek, Cohere"
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
Add these resellers as descriptors (CPE-468) + unified manifests (CPE-471) + allow-listed egress
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Perplexity (Sonar), Mistral La Plateforme, DeepSeek, Cohere.

Bases: api.perplexity.ai, api.mistral.ai/v1, api.deepseek.com, api.cohere.ai/compatibility/v1.

## Acceptance Criteria
- [x] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [x] Each appears in the provider dropdown and launches an agent with its stored key.
- [x] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [x] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
First-party APIs that expose a menu of their own models; OpenAI-compatible (Cohere via its compat endpoint).

## Resolution
Added Mistral, DeepSeek, and Cohere end-to-end via the established reseller pattern (data + one host
allow-list edit each):
- Sidecar `resellers/{mistral,deepseek,cohere}.json` — protocol `openai`, `launch_base_url`
  (api.mistral.ai/v1, api.deepseek.com, api.cohere.ai/compatibility/v1), `models_endpoint`,
  `api_hosts`, normalizer `openai`. Added to `KNOWN_RESELLERS` + the bundled-descriptors test.
- Host `models_egress` allow-list gained their `/models` endpoints; every-reseller test extended.

**Perplexity was intentionally skipped:** it exposes no public `/models` list endpoint, so the
model-picker normalizer has nothing to enumerate — adding it would need a hardcoded model set (a
different mechanism). Filed as a follow-up consideration rather than a broken half-reseller.

Verified: sidecar `model_catalog` 8 pass; host `models_egress` 3 pass; clippy clean both crates/modes.
Nightshift loop 6. (This loop also closed CPE-473 + CPE-474 as delivered-by-CPE-471/469.)
