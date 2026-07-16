---
id: CPE-468
title: "Data-driven reseller-provider recipe (openai- & anthropic-compatible)"
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
A single parameterized launch recipe **per protocol** (openai-compatible, anthropic-compatible) that
any agent can target against any reseller, driven by a reseller **descriptor** (base_url, auth style,
env-var mapping, protocol). Adding a reseller becomes data, not a per-agent recipe. Extends the
routing engine (CPE-285); `openrouter` becomes the first descriptor of the anthropic-compatible shape.

## Acceptance Criteria
- [x] A reseller descriptor `{ id, name, protocol, base_url }` drives launch
      (`routing::ResellerDescriptor` + `compose_reseller_launch`).
- [x] Protocol templates are **agent-declared data** (`AgentManifest.reseller_recipes`, keyed by
      protocol, using `{base_url}`/`{api_key}`/`{model}`) — so `anthropic`/`openai`/any dialect is
      supported by the same mechanism, not hardcoded. A reseller supplies `base_url`.
- [x] An agent gains reseller support by declaring ONE `reseller_recipes[protocol]` entry; after that,
      ANY reseller of that protocol works purely as data (proven by two descriptors, same recipe).
- [x] Existing `openrouter` provider-recipe launch behaviour is unchanged (its `provider_recipes` path
      is untouched; the 6 original routing tests still pass).
- [x] Unit tests for the reseller path; clippy `--all-targets -D warnings` clean.

## Resolution
Extended the routing engine (CPE-285) with a **data-driven reseller mechanism**:
- `AgentManifest.reseller_recipes: BTreeMap<protocol, ProviderRecipe>` — how an agent consumes ANY
  reseller of a protocol, as `{base_url}`/`{api_key}` templates. Plus `reseller_protocols()` /
  `supports_reseller()` so the launcher can offer matching resellers.
- `routing::ResellerDescriptor { id, name, protocol, base_url }` + `compose_reseller_launch()` — sets
  `{base_url}` from the reseller (it wins — selecting a reseller means using its endpoint) and fills
  the agent's protocol recipe. Refactored the shared fill into `apply_recipe()` (DRY with
  `compose_launch`). Exported both from the crate.
- **4 new tests** (descriptor → launch, a second reseller with no code change = the "add as data"
  proof, unsupported-protocol rejection, missing-key loud error). `cargo test -p ai-console` 188
  passed; clippy `--all-targets -D warnings` clean.

This is the epic keystone: the launcher wiring (CPE-469), egress allow-list (CPE-470), unified
reseller manifests + migrating `openrouter` to a descriptor (CPE-471), and the reseller batches
(CPE-473–478) all build on this. Nightshift loop 2.

## Notes
Foundation for the epic. The current per-agent `provider_recipes` stays supported for overrides.
