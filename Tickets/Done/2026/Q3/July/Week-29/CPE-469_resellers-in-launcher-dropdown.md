---
id: CPE-469
title: "Resellers in the launcher provider dropdown + reseller-key wiring to launch"
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
Surface every configured reseller in the launcher provider dropdown, and pass the reseller’s stored
key (CPE-452 reseller-key broker) into the launch. Uniform inline control (per prefer-inline-instant).

## Acceptance Criteria
- [x] Provider dropdown lists the agent's providers + every reseller whose protocol the agent speaks (CPE-469).
- [x] Selecting a reseller routes handle_launch through compose_reseller_launch (server matches the id);
      the reseller key resolves from the vault, model picker already filters by the reseller id.
- [x] The Model picker already filters by reseller — the reseller id IS the provider value, so it drives it.
- [x] Launcher jsdom test: matching resellers appear as options; non-matching protocol excluded. Plus a
      scope test proving build_launch fills OPENAI_BASE_URL from the reseller for the real qwen manifest.

## Resolution
Wired resellers end-to-end as selectable launch providers (the epic payoff):
- **Routing/scope:** `AgentLaunchRequest` gained `reseller: Option<ResellerDescriptor>`; `build_launch`
  branches to `compose_reseller_launch` when set.
- **Console:** `ConsoleState` loads reseller descriptors (`with_resellers`, from the bundled
  `resellers/` dir via `ResellerRegistry::descriptors()`); `handle_launch` sets `reseller` when the
  selected provider id matches a known reseller AND the agent speaks its protocol. `/api/catalog` now
  exposes `resellers: [{id,name,protocol}]` + each agent's `resellerProtocols`.
- **Agents:** `qwen` + `codex` gained an `openai` `reseller_recipes` entry (OPENAI_BASE_URL/API_KEY
  for qwen; `-c model_providers` config for codex).
- **Launcher:** `renderProviders` appends every reseller whose protocol the agent speaks as a
  "<name> (reseller)" option; selecting it drives the model picker (id = reseller) + the launch.
- Tests: scope reseller-launch (real qwen manifest), launcher jsdom dropdown filtering. `cargo test`
  192 passed; clippy `--all-targets -D warnings` clean; `npm run check` clean; 33 launcher tests pass.

The 6 migrated OpenAI resellers (together/fireworks/groq/deepinfra/novita/aimlapi) are now selectable
for OpenAI-compatible agents. Remaining epic: CPE-470 (host egress allow-list from reseller hosts),
CPE-473–478 (more resellers as data), CPE-479/480 (docs + conformance). Nightshift loop 4.
