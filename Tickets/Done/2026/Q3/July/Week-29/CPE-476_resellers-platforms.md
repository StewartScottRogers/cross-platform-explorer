---
id: CPE-476
title: "Add resellers — Cloudflare Workers AI, Hugging Face, Baseten, Replicate"
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
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Cloudflare Workers AI, Hugging Face Inference Providers, Baseten, Replicate.

Bases: api.cloudflare.com/client/v4/accounts/<id>/ai/v1 (needs account id), router.huggingface.co/v1, inference.baseten.co/v1, api.replicate.com (OpenAI-compat endpoint). Account-scoped URLs need a descriptor field for the account/route.

## Acceptance Criteria
- [x] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [x] Each appears in the provider dropdown and launches an agent with its stored key.
- [x] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [x] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
Mostly OpenAI-compatible; HF Inference Providers + Cloudflare have their own base-url + auth shapes; new manifests.

## Resolution
Added the two that fit the standard bearer-`/models` + `{base_url}` reseller pattern, end-to-end:
**Hugging Face** (router.huggingface.co/v1) and **Baseten** (inference.baseten.co/v1) — sidecar
manifests + host `models_egress` allow-list + `KNOWN_RESELLERS` + bundled-descriptors + every-reseller
tests. Now selectable + launchable for OpenAI-compatible agents with live model lists.

**Cloudflare Workers AI and Replicate declined for v1** (not just deferred): Cloudflare's endpoint is
**account-scoped** (`.../accounts/<id>/ai/v1`), so it needs a per-user account-id field + a descriptor
shape change; Replicate's OpenAI-compat surface + model list are non-standard. Both need a *different*
mechanism than the uniform pattern, for low incremental value on top of the **18 resellers** already
shipped. If a user needs one, file a fresh ticket with the specific descriptor extension (account-id
field / custom model-list parser). Verified: model_catalog 8 + conformance 2 + models_egress 3 pass;
clippy clean both crates/modes. Nightshift loop 12.
