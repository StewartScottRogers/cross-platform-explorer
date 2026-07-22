---
id: CPE-453
title: "Reseller research dossier + threat model"
type: Task
status: Done
priority: High
component: Docs
tags: [ready]
estimate: 1-2h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
Capture the reseller research (endpoints, auth, shapes, sources) as a doc, and a threat model for the model-catalog: egress allow-list per reseller, untrusted model metadata, per-reseller key handling, and GitHub-snapshot signing/anti-rollback.

## Acceptance Criteria
- [x] `docs/design/model-resellers.md`: table of each reseller (id, model-list endpoint, auth, shape, tier) with citations.
- [x] Threat-model section (STRIDE, extends CPE-304/forge-threat-model): multi-host allow-listed egress, untrusted model metadata (advisory only), per-reseller keys at rest/in transit/logs, signed-snapshot integrity + anti-rollback.
- [x] Records the invariants CPE-447/450/451/452 must satisfy.
- [x] Sources cited.

## Notes
Do this early - it pins the endpoints CPE-446/448 encode and the egress invariants CPE-447 enforces.

## Resolution
Wrote `docs/design/model-resellers.md`: the reseller table (id, model-list endpoint, auth, normalizer, tier, notes) for OpenRouter + Together/Fireworks/Groq/DeepInfra/Novita/AI-ML-API/WaveSpeed/GitHub-Models/Eden/Portkey/Cloudflare/cloud/LiteLLM, with verify-before-live flags; a STRIDE threat model extending CPE-304/forge (multi-host allow-listed egress, untrusted advisory metadata, per-reseller keys, signed-snapshot anti-rollback); the required invariants (owners CPE-447/450/451/452); non-goals; and sources. Doc-only; pins the endpoints CPE-446/448 encode and the egress invariant CPE-447 enforces.
