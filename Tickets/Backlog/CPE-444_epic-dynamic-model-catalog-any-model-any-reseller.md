---
id: CPE-444
title: "EPIC: Dynamic model catalog — select any model from any reseller"
type: Task
status: Open
priority: High
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-15
---

## Summary
Let the AI Console pick **any** model an aggregator/reseller offers — starting with **OpenRouter's**
full list (300+ models) — instead of the small hand-curated set baked into agent manifests today. The
list changes constantly, so it must be **dynamically regenerated on a schedule**, cached, and kept
**offline-usable**. Because more model resellers keep appearing, model access is generalised behind a
**reseller registry** (declarative manifests, exactly like the forge providers CPE-430 and the agent
catalog CPE-278): adding a reseller is data, not host code.

## Motivation
Today a provider's models are effectively fixed in each agent's `provider_recipes`. OpenRouter alone
adds/removes models weekly with rich metadata (context length, price, modality, moderation). Users
want to choose the current best/cheapest model without waiting for an app release. And OpenRouter is
one of many — the same "unified API + a `/models` listing endpoint" shape is now offered by a dozen
resellers, so the design should onboard any of them declaratively.

## Reseller research (2026 landscape)
The common pattern is a unified, usually **OpenAI-compatible** API with a **dynamic model-list
endpoint**. Candidates, roughly tiered:

**Tier 1 — unified resellers with a rich dynamic model list (OpenAI-compatible `/v1/models`):**
- **OpenRouter** — `GET https://openrouter.ai/api/v1/models` — richest metadata (pricing, context,
  modalities, per-model provider). **The first-class source.**
- **Together AI** — `GET https://api.together.xyz/v1/models`
- **Fireworks AI** — `GET https://api.fireworks.ai/inference/v1/models`
- **Groq** — `GET https://api.groq.com/openai/v1/models` (low-latency)
- **DeepInfra** — `GET https://api.deepinfra.com/v1/openai/models`
- **Novita AI** — `GET https://api.novita.ai/v3/openai/models`
- **AI/ML API** — `GET https://api.aimlapi.com/models` (400+)
- **WaveSpeedAI** — OpenAI-compatible, 290+

**Tier 2 — different catalog shape (needs its own normalizer):**
- **GitHub Models** — `GET https://models.github.ai/catalog/models` (publisher/modality/rate-limits;
  ties directly to the "keep it on GitHub" idea). Auth: GitHub token, `X-GitHub-Api-Version`.
- **Eden AI** — 500+ models incl. multimodal (aggregator, own shape)
- **Cloudflare Workers AI / AI Gateway** — model list + gateway routing
- **Portkey** — AI gateway, 1,600+ via virtual keys/config (gateway, not a flat list)

**Tier 3 — cloud model catalogs (heavier auth, future):** AWS Bedrock, Azure AI Foundry, Google
Vertex Model Garden.

**Self-hosted gateway (advanced):** LiteLLM proxy (`/models`) — a user-run unified endpoint.

Sources: OpenRouter/Together/Fireworks/Groq model-list docs; GitHub Models catalog API
(`https://models.github.ai/catalog/models`); OpenRouter-alternatives roundups (Eden AI, Portkey,
LLM API, AnyAPI) — see CPE-453 for the captured research + citations.

## Architecture
- **Model shape** — one provider-agnostic `Model { id, reseller, display_name, context_length,
  pricing, modalities, capabilities, moderated, … }` the picker renders and the launch flow consumes.
- **Reseller registry** — declarative `reseller manifest` per source: `{id, name, models_endpoint,
  auth, api_hosts, normalize}`. Adding a reseller = drop a manifest (mirrors CPE-430 forge providers
  + CPE-278 agents). The union of `api_hosts` is the egress allow-list.
- **Dynamic refresh** — fetch each reseller's model list on a cadence + on demand; normalize → the
  common shape. **Host-brokered, allow-listed egress** (no SSRF; reuse CPE-347/keyverify + CPE-433
  patterns); offline/proxy honoured (CPE-369).
- **GitHub-hosted snapshot** — a scheduled job regenerates a normalized, **signed** model-catalog
  bundle and publishes it to the app's GitHub Releases; clients **download + verify + hot-reload** it
  (reuse the agent-catalog update pipeline CPE-308/376/371). This keeps clients fast + offline-capable
  and avoids every client hammering every reseller API.
- **UI** — a **searchable, filterable** model picker (300+ entries) in the AI Console launcher, with
  metadata (context/price/modalities) and reseller filter.
- **Credentials** — per-reseller keys via the secrets broker (reuse CPE-344/348).

## Child tickets
- **CPE-445** — Model-catalog data model + reseller-manifest registry (declarative; OpenRouter first).
- **CPE-446** — OpenRouter model source: fetch + normalize `/api/v1/models` → the common `Model`.
- **CPE-447** — Host-brokered, allow-listed model-list egress (no SSRF; offline/proxy).
- **CPE-448** — Reseller manifest catalog: Together, Fireworks, Groq, DeepInfra, Novita, AI/ML API,
  WaveSpeed, GitHub Models (+ normalizers where the shape differs).
- **CPE-449** — Dynamic model picker UI: searchable/filterable full list with metadata.
- **CPE-450** — Scheduled regeneration → signed model-catalog snapshot on GitHub Releases (reuse 308/376).
- **CPE-451** — Client fetch + verify + hot-reload of the snapshot; offline/stale + refresh cadence.
- **CPE-452** — Per-reseller credentials via the secrets broker + key entry/verify.
- **CPE-453** — Reseller research dossier + threat model (egress allow-list, untrusted model metadata,
  key handling, GitHub-snapshot signing/anti-rollback).

## Design decisions
- **D1 — OpenAI-compatible `/v1/models` first.** Normalize that shape once; per-reseller manifests add
  auth + host + field-mapping. GitHub Models / Eden get bespoke normalizers (Tier 2).
- **D2 — Signed GitHub snapshot is the default source; live reseller fetch is opt-in/refresh.** Reuse
  the existing signed-catalog verify + anti-rollback (CPE-308/371) — dormant until a catalog signing
  key exists (same gate as CPE-308). Offline always works from the last good snapshot.
- **D3 — Reseller = data, not code.** Manifests only; a new reseller ships without host changes.
- **D4 — Egress stays host-brokered + allow-listed.** The sidecar never supplies a URL; the host maps
  reseller→endpoint from the manifest (same guarantee as CPE-347/433).
- **D5 — Model metadata is untrusted display data.** Parsed defensively; never executed; pricing/limits
  are advisory. Keys are per-reseller, keychain-backed, never logged (Redactor).

## Acceptance Criteria (epic — closes when children do)
- [ ] The AI Console can select any model OpenRouter lists, from a dynamic, refreshable catalog.
- [ ] At least one more reseller is onboarded purely by manifest (proving D3).
- [ ] The list regenerates on a schedule and is available offline from a signed GitHub snapshot.
- [ ] Egress is host-brokered + allow-listed; per-reseller keys are keychain-backed; threat model recorded.

## Notes
Relates to the AI Console epic (CPE-261) and reuses three existing systems: the agent-catalog update
pipeline (CPE-308/376/371), host-brokered egress (CPE-347/433), and the secrets broker (CPE-344/348).
Not a single unit of work — tracks its children.
