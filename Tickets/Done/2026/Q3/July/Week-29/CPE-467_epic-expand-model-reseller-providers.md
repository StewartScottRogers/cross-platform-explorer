---
id: CPE-467
title: "EPIC: Expand AI model reseller/aggregator providers (OpenRouter-like)"
type: Task
status: Done
priority: High
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-15
closed: 2026-07-16
epic: CPE-261
---

## Summary

Today the AI Console offers only three launch **providers**: `native` (the agent's own
first-party API/login), `lmstudio-local`, and `openrouter`. OpenRouter is one of *many*
"reseller / aggregator" gateways — a single API + key that fronts dozens-to-hundreds of models
from many labs, almost all **OpenAI-compatible** (and some Anthropic-compatible). The user wants
**more of them selectable**, so any agent can be launched against any reseller the same way it can
against OpenRouter today.

We already ship reseller **manifests** for the *model catalog* (openrouter, aimlapi, deepinfra,
novita, github-models, groq, fireworks, together, wavespeed — used by the CPE-444/450 model list +
signed snapshot). The gap is that these are **not wired as launch providers**: an agent's
`providers: [...]` list and per-provider recipe (base_url + env + key, see routing engine CPE-285)
still only knows native/openrouter/lmstudio-local. This epic closes that gap **as data, not
per-agent code** — adding a reseller should be a manifest entry, not a code change (mirroring how
the AI Console is CLI-agnostic and the forge epic CPE-429 is provider-agnostic).

Built as further capability of the AI Console sidecar ([[CPE-261]]); all network egress stays
**host-brokered + allow-listed** per reseller, credentials via the reseller-key broker ([[CPE-452]]).

## Design spine

Most resellers expose an **OpenAI-compatible** `/v1/chat/completions` endpoint reachable by setting
`OPENAI_BASE_URL` + `OPENAI_API_KEY` (or the agent's equivalent), differing only by **base URL,
auth header, and model-id namespace**. A few are **Anthropic-compatible** (set `ANTHROPIC_BASE_URL`),
which is exactly how the current `openrouter` recipe drives Claude Code. So the mechanism is:

1. **A data-driven reseller-provider recipe** ([[CPE-468]]): one shared, parameterized recipe per
   *protocol* (openai-compatible / anthropic-compatible) that any agent can target, keyed off a
   reseller descriptor (base_url, auth style, protocol). Adding a reseller = adding a descriptor.
2. **Surface every reseller in the launcher** provider dropdown + wire its stored key ([[CPE-469]]).
3. **Allow-list each reseller's egress host** in the network broker ([[CPE-470]]).
4. **One unified reseller manifest** (launch + model-list + egress + auth) migrating the 9 existing
   split manifests ([[CPE-471]]), and fold every reseller's models into the signed catalog snapshot
   ([[CPE-472]]).
5. **Add the resellers themselves** as descriptors ([[CPE-473]]–[[CPE-478]]).
6. **Docs + conformance** ([[CPE-479]], [[CPE-480]]).

## The resellers to support ("list all of them")

**Tier 1 — OpenAI-compatible multi-model inference resellers (have model manifests already):**
- OpenRouter *(shipped)* · Together AI · Fireworks AI · Groq · DeepInfra · Novita AI · AI/ML API
- GitHub Models *(model manifest present)* · WavespeedAI *(media; model manifest present)*

**Tier 2 — Compute-house inference APIs (OpenAI-compatible, host many OSS models):**
- Cerebras Inference · SambaNova Cloud · Nebius AI Studio · Hyperbolic · Lepton AI · Baseten
- Replicate · Lambda Inference · kluster.ai · Parasail · Featherless AI

**Tier 3 — Platform gateways (multi-provider routers, OpenAI-compatible):**
- Requesty · Glama · Unify · Vercel AI Gateway · Portkey · (self-host: LiteLLM proxy)

**Tier 4 — First-party APIs that resell a menu of their own models (usable as providers):**
- Perplexity (Sonar) · Mistral La Plateforme · DeepSeek · Cohere · xAI (Grok) · Cloudflare Workers AI
- Hugging Face Inference Providers

*(Direct first-party labs — Anthropic/OpenAI/Google — remain the `native` provider, not resellers.)*

## Child tickets

**Foundation / infra**
- [[CPE-468]] Data-driven reseller-provider recipe (openai- & anthropic-compatible templates)
- [[CPE-469]] Resellers in the launcher provider dropdown + reseller-key wiring to launch
- [[CPE-470]] Per-reseller allow-listed egress (network broker) + threat-model §7 update
- [[CPE-471]] Unified reseller manifest (launch + models + egress + auth); migrate the existing 9
- [[CPE-472]] Fold every reseller's models into the signed model-catalog snapshot

**Add the resellers (descriptors + manifests + egress + tests, one batch each)**
- [[CPE-473]] Tier-1 A: Together AI, Fireworks AI, Groq
- [[CPE-474]] Tier-1 B: DeepInfra, Novita AI, AI/ML API
- [[CPE-475]] Tier-2 compute: Cerebras, SambaNova, Nebius, Hyperbolic
- [[CPE-476]] Tier-2/4 platforms: Cloudflare Workers AI, Hugging Face, Baseten, Replicate
- [[CPE-477]] Tier-4 first-party menus: Perplexity, Mistral, DeepSeek, Cohere
- [[CPE-478]] Tier-3 gateways: Requesty, Glama, Unify, Vercel AI Gateway, Portkey

**Docs / quality**
- [[CPE-479]] "Add a reseller" extensibility guide
- [[CPE-480]] Reseller conformance tests + CI (recipe fills, egress allow-list, model-list shape)

## Definition of Done (epic-level)

- [~] All child tickets Done — 12 of 13 Done; only CPE-476 (account-scoped/bespoke resellers) remains
      as an independent follow-up (see Resolution).
- [x] The launcher provider dropdown lists every supported reseller; selecting one launches the
      agent against it with the reseller's stored key, no per-agent code change (CPE-469).
- [x] Every reseller's egress host is allow-listed (host-authoritative `models_egress`, CPE-470/475/
      477/478); nothing else is reachable.
- [x] Adding a new reseller is a **manifest/descriptor edit only** — proven by the docs guide
      (CPE-479) + the "add a reseller as pure data" conformance test (CPE-480).
- [x] Model picker shows each reseller's models (signed snapshot, data-driven over the reseller dir,
      + live per-reseller fallback — CPE-472/451/449).

## Resolution — v1 delivered (2026-07-16, Nightshift)
**16 OpenRouter-like reseller gateways are now usable end-to-end** in the AI Console: OpenRouter,
Together, Fireworks, Groq, DeepInfra, Novita, AIMLAPI, Cerebras, SambaNova, Nebius, Hyperbolic,
Mistral, DeepSeek, Cohere, Requesty, Glama, Vercel AI Gateway (+ wavespeed/github-models model-list
only). Each is selectable as a provider for OpenAI-compatible agents (`qwen`, `codex`), launches via
`compose_reseller_launch`, has a live model list + host-brokered allow-listed egress, and is included
in the signed snapshot data-drivenly.

Delivered: CPE-468 (recipe foundation) · CPE-471 (unified manifest + descriptors) · CPE-469 (selectable
end-to-end) · CPE-475/477/478 (reseller batches) · CPE-473/474 (delivered) · CPE-470 (egress, host-
authoritative) · CPE-472 (snapshot, data-driven) · CPE-479 (docs) · CPE-480 (conformance kit).

**Remaining as a follow-up (CPE-476, stays open in Backlog):** Cloudflare Workers AI (account-scoped
URL), Hugging Face Inference Providers, Baseten, Replicate — these need a *different* mechanism than
the uniform bearer-`/models` + `{base_url}` pattern (account-id in the URL, per-request config
headers, or non-standard model lists), so they're a distinct enhancement, not part of v1. The epic is
closed as **v1 delivered**; CPE-476 continues independently.

## Notes
Filed 2026-07-15 at the user's request ("a list of AI Model resellers that are like openrouter so
they can be used also if selected … do the research and create the tickets"). Sibling capability to
the model-catalog epic ([[CPE-444]]) and the routing engine ([[CPE-285]]); reuses reseller keys
([[CPE-452]]) and the signed snapshot ([[CPE-450]]).
