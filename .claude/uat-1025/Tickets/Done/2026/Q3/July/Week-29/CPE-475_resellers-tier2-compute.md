---
id: CPE-475
title: "Add resellers — Cerebras, SambaNova, Nebius, Hyperbolic"
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
(CPE-470), so each is selectable as a launch provider and its models appear in the picker: Cerebras Inference, SambaNova Cloud, Nebius AI Studio, Hyperbolic.

Bases: api.cerebras.ai/v1, api.sambanova.ai/v1, api.studio.nebius.ai/v1, api.hyperbolic.xyz/v1. Cerebras/SambaNova are ultra-low-latency; Nebius/Hyperbolic host broad OSS menus.

## Acceptance Criteria
- [x] Each reseller has a unified manifest (protocol `openai` + `launch_base_url` + `api_hosts` +
      `models_endpoint`, auth bearer) → a launch descriptor, plus a host egress allow-list entry.
- [x] Each appears in the provider dropdown for OpenAI-compatible agents and launches via
      `compose_reseller_launch` (CPE-469 mechanism) with its stored reseller key.
- [x] Each reseller's model list resolves live (host `models_egress` allow-list extended). Inclusion
      in the signed snapshot is CPE-472 (the snapshot workflow iterates the reseller manifests, so
      these are picked up when it next runs).
- [x] Tests: sidecar bundled-resellers descriptor test + host `models_egress` every-reseller test both
      extended to the 4 new ids; clippy `--all-targets -D warnings` clean in both host feature modes.

## Resolution
Added 4 OpenAI-compatible compute resellers **end-to-end as data + one host allow-list edit** each:
- **Sidecar:** `resellers/{cerebras,sambanova,nebius,hyperbolic}.json` (protocol `openai`,
  `launch_base_url` = their `/v1` base, `models_endpoint`, `api_hosts`, normalizer `openai`). Added to
  `console.rs` `KNOWN_RESELLERS` (Keys panel) + the `model_catalog` bundled-descriptors test.
- **Host:** `models_egress::models_endpoint` gained the 4 `/models` endpoints (the host-authoritative
  SSRF allow-list — deliberately NOT derived from the sidecar manifests); the every-reseller test
  covers them.

Bases: api.cerebras.ai/v1, api.sambanova.ai/v1, api.studio.nebius.ai/v1, api.hyperbolic.xyz/v1.
Verified: sidecar `model_catalog` 8 tests pass; host `models_egress` 3 tests pass; clippy clean
(sidecar + host both feature modes). Nightshift loop 5.

## Notes
OpenAI-compatible compute-house inference APIs hosting many OSS models; new manifests needed.
Note: the host `models_egress` allow-list stays **hardcoded/authoritative** (the SSRF boundary) rather
than derived from sidecar manifests — so CPE-470's "derive from descriptors" framing should be revised
to "keep the host allow-list in sync when adding resellers" (done here).
