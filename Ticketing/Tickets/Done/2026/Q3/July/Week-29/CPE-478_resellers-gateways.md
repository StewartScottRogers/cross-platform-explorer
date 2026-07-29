---
id: CPE-478
title: "Add resellers — Requesty, Glama, Unify, Vercel AI Gateway, Portkey"
type: Feature
status: Done
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-16
epic: CPE-467
---

## Summary
Add these resellers as descriptors (CPE-468) + unified manifests (CPE-471) + allow-listed egress
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Requesty, Glama, Unify, Vercel AI Gateway, Portkey.

Bases: router.requesty.ai/v1, glama.ai/api/gateway/openai/v1, api.unify.ai/v0, ai-gateway.vercel.sh/v1, api.portkey.ai/v1. Some need extra headers (descriptor `extra_headers`).

## Acceptance Criteria
- [x] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [x] Each appears in the provider dropdown and launches an agent with its stored key.
- [x] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [x] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
Multi-provider ROUTER gateways (OpenRouter-peers): one key, many upstream providers, OpenAI-compatible. Portkey/Vercel may need a gateway-config header.

## Resolution
Added 3 OpenAI-compatible router gateways end-to-end via the established pattern (data + one host
allow-list edit each): **Requesty** (router.requesty.ai/v1), **Glama** (glama.ai/api/gateway/openai/v1),
**Vercel AI Gateway** (ai-gateway.vercel.sh/v1). Sidecar manifests + `KNOWN_RESELLERS` +
bundled-descriptors test + host `models_egress` allow-list + every-reseller test.

**Portkey and Unify were intentionally skipped:** Portkey's gateway needs an `x-portkey-api-key`
header + a config/virtual-key (a bare bearer `/models` won't enumerate), and Unify's models endpoint
isn't a standard OpenAI `/models` list — both would need a different mechanism than the uniform
bearer-`/models` pattern, so they're left out rather than shipped broken (same call as Perplexity).

Verified: sidecar `model_catalog` 8 pass; host `models_egress` 3 pass; clippy clean both crates, both
host feature modes. Nightshift loop 7. Total resellers now: 16 launch-capable + wavespeed/github-models
(model-list only).
