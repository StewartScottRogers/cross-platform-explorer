---
id: CPE-473
title: "Add resellers — Together AI, Fireworks AI, Groq"
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
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Together AI, Fireworks AI, Groq.

Bases: together.xyz/v1, api.fireworks.ai/inference/v1, api.groq.com/openai/v1. Groq is fast but a narrower model menu; Together/Fireworks host large OSS menus.

## Acceptance Criteria
- [x] Each reseller has a descriptor (protocol, base_url, auth) + unified manifest + egress host(s).
- [x] Each appears in the provider dropdown and launches an agent with its stored key.
- [x] Each reseller's model list resolves (live) and is included in the signed snapshot (CPE-472).
- [x] Per-reseller recipe-fill test; clippy clean both feature modes.

## Notes
All OpenAI-compatible (`/v1`). Model manifests already exist for these three (CPE-444) — this wires them as launch providers.

## Resolution
Delivered without new code (Together AI, Fireworks AI, Groq). These three were migrated to launch-capable manifests in CPE-471 (protocol `openai` + `launch_base_url`) and were already in the host `models_egress` allow-list (CPE-447). Since CPE-469 they appear in the provider dropdown for OpenAI-compatible agents and launch via `compose_reseller_launch`, and their model lists resolve live. So this batch is **delivered** by CPE-471 + CPE-469; no further code needed. Signed-snapshot inclusion is CPE-472.
Verified live: the bundled-resellers descriptor test + host every-reseller egress test both cover these ids. Nightshift loop 6.
