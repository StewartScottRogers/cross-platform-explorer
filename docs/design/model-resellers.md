# Model resellers — research dossier + threat model (CPE-453)

Supports the dynamic model catalog (CPE-444): let the AI Console select **any** model a reseller
offers, kept fresh via a signed GitHub snapshot. This dossier captures **who** the resellers are and
**how** to reach their model lists, and the **threat model** for pulling that data + keys safely.

## What a "reseller" is here

A service that exposes **many** models behind **one** API, with a **dynamic model-list endpoint** we
can enumerate. Most are **OpenAI-compatible** (`GET …/v1/models`), which is why one normalizer covers
several. The catalog treats each as a declarative manifest (`sidecar/ai-console/resellers/*.json`,
CPE-445/448); adding one is data, not host code.

## Reseller table

| id | Model-list endpoint | Auth | Shape / normalizer | Tier | Notes |
|----|--------------------|------|--------------------|------|-------|
| **openrouter** | `https://openrouter.ai/api/v1/models` | Bearer | rich (pricing/context/modalities) → `openrouter` | 1 | **First-class source**; ~300+ models, richest metadata |
| together | `https://api.together.xyz/v1/models` | Bearer | OpenAI-ish → `openai` | 1 | open-model host |
| fireworks | `https://api.fireworks.ai/inference/v1/models` | Bearer | OpenAI → `openai` | 1 | fast inference |
| groq | `https://api.groq.com/openai/v1/models` | Bearer | OpenAI → `openai` | 1 | low-latency (LPU) |
| deepinfra | `https://api.deepinfra.com/v1/openai/models` | Bearer | OpenAI → `openai` | 1 | verify path before live |
| novita | `https://api.novita.ai/v3/openai/models` | Bearer | OpenAI → `openai` | 1 | verify path before live |
| aimlapi | `https://api.aimlapi.com/v1/models` | Bearer | OpenAI → `openai` | 1 | 400+; verify path before live |
| wavespeed | `https://api.wavespeed.ai/v1/models` | Bearer | OpenAI → `openai` | 1 | 290+; **endpoint to verify** |
| **github-models** | `https://models.github.ai/catalog/models` | GitHub token + `X-GitHub-Api-Version` | catalog (publisher/modality/rate-limit) → `github` | 2 | ties to the "keep it on GitHub" idea |
| eden-ai | (aggregator API) | key | own shape → bespoke | 2 | 500+ incl. multimodal; future |
| portkey | (gateway/virtual keys) | key | gateway config, not a flat list | 2 | 1,600+ via gateway; future |
| cloudflare | (Workers AI / AI Gateway) | key | own shape | 2 | future |
| bedrock / azure / vertex | cloud catalogs | cloud IAM | own shapes | 3 | heavier auth; future |
| litellm | user's `…/v1/models` | user | OpenAI → `openai` | self-host | a user-run unified proxy |

> **Endpoint verification:** the exact paths for deepinfra/novita/aimlapi/wavespeed are best-known and
> **must be confirmed against live docs before CPE-450 (the scheduled fetch) enables them**. They pass
> manifest validation (https host + known normalizer) but are marked here as verify-before-live.

## Threat model (STRIDE — extends CPE-304 + `forge-threat-model.md`)

**New surfaces vs. the AI Console's single key-check:** many outbound model-list hosts; untrusted
model **metadata** landing in the picker; per-reseller **keys**; and a **signed GitHub snapshot** as
the default source.

| Surface | Threat | Mitigation | Status |
|---------|--------|-----------|--------|
| Multi-host egress (model-list fetch) | SSRF — sidecar coerces host into fetching an attacker URL. | Sidecar sends `{reseller}`, never a URL; host maps reseller→endpoint from a **host-authoritative allow-list** (`models_egress`, CPE-447) = union of manifest `api_hosts`; refuses any other host. No general fetch. | 🟡 CPE-447 (allow-list ✅) |
| Egress (transport) | Key leaks / MITM. | TLS only; token in the reseller's auth header, only to its allow-listed host; `Redactor` scrubs logs; offline/proxy honoured (CPE-369). | 🟡 CPE-447 |
| Untrusted model metadata | A hostile/compromised reseller returns crafted names/pricing to mislead or XSS the picker. | Parsing is **pure + total** (`parse_*` → empty on malformed, no panic); metadata is **advisory display data**, never executed; the picker renders it as text (no HTML injection); pricing/limits are non-authoritative. | ✅ (parser CPE-446) |
| Per-reseller keys | Key at rest / in logs / cross-tenant. | Keychain via the secrets broker, per-reseller namespace (`models/<reseller>/<label>`, CPE-452); never in config/logs (Redactor); a sidecar addresses only its own keys. | 🟡 CPE-452 |
| Signed GitHub snapshot | A tampered/stale-but-signed model bundle. | ed25519-verified against a trusted key, content-hashed, **strictly-monotonic version** (anti-rollback), host-mediated fetch from the app's Releases — reuse CPE-308/371/376 exactly. | ⛔ CPE-450/451 (pipeline exists; dormant until a signing key, same gate as CPE-308) |
| Availability | Offline / a slow reseller. | Offline ⇒ no call; serve the last good snapshot with an "as of <date>" note; per-request timeout; a stall is contained to the servicing thread. | 🟡 CPE-447/451 |

## Required invariants (build requirements)

| Invariant | Owner | State |
|-----------|-------|-------|
| Sidecar never supplies a URL; egress host ∈ reseller allow-list. | CPE-447 | 🟡 (allow-list ✅) |
| Model metadata parsing is total + non-executing; rendered as inert text. | CPE-446/449 | ✅ parser |
| Per-reseller keys keychain-backed, Redactor-scrubbed, namespaced. | CPE-452 | ⛔ |
| Snapshot ed25519-verified + anti-rollback before use; offline falls back to last good. | CPE-450/451 | ⛔ (shares CPE-308 key gate) |

## Non-goals / accepted risks

- Pricing/context/limits are **advisory** (reseller-reported), not a billing source of truth.
- Tier-2/3 resellers (Eden, Portkey, Cloudflare, cloud catalogs) are **future** — the registry is
  ready for them, but their bespoke normalizers/auth aren't built here.
- The catalog does not *validate* that a listed model actually works for a given key — that surfaces
  at inference time.

## Sources

- OpenRouter models API (`/api/v1/models`); Together / Fireworks / Groq / DeepInfra / Novita
  OpenAI-compatible `/models` docs.
- GitHub Models catalog API — `https://models.github.ai/catalog/models` (GitHub REST `models/catalog`).
- OpenRouter-alternatives roundups (Eden AI, Portkey, LLM API, AnyAPI, AI/ML API) for the reseller
  landscape and model counts.
