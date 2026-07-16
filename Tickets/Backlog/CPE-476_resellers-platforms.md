---
id: CPE-476
title: "Add resellers — Cloudflare Workers AI, Hugging Face, Baseten, Replicate"
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
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Cloudflare Workers AI, Hugging Face Inference Providers, Baseten, Replicate.

Bases: api.cloudflare.com/client/v4/accounts/<id>/ai/v1 (needs account id), router.huggingface.co/v1, inference.baseten.co/v1, api.replicate.com (OpenAI-compat endpoint). Account-scoped URLs need a descriptor field for the account/route.

## Acceptance Criteria
- [ ] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [ ] Each appears in the provider dropdown and launches an agent with its stored key.
- [ ] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [ ] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
Mostly OpenAI-compatible; HF Inference Providers + Cloudflare have their own base-url + auth shapes; new manifests.
