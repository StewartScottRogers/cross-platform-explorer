---
id: CPE-474
title: "Add resellers — DeepInfra, Novita AI, AI/ML API"
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
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: DeepInfra, Novita AI, AI/ML API (aimlapi).

Bases: api.deepinfra.com/v1/openai, api.novita.ai/v3/openai, api.aimlapi.com/v1.

## Acceptance Criteria
- [x] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [x] Each appears in the provider dropdown and launches an agent with its stored key.
- [x] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [x] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
OpenAI-compatible. Model manifests already exist (CPE-444) — wire as launch providers.

## Resolution
Delivered without new code (DeepInfra, Novita AI, AI/ML API). These three were migrated to launch-capable manifests in CPE-471 (protocol `openai` + `launch_base_url`) and were already in the host `models_egress` allow-list (CPE-447). Since CPE-469 they appear in the provider dropdown for OpenAI-compatible agents and launch via `compose_reseller_launch`, with live model lists. **Delivered** by CPE-471 + CPE-469. Signed-snapshot inclusion is CPE-472.
Verified live: the bundled-resellers descriptor test + host every-reseller egress test both cover these ids. Nightshift loop 6.
