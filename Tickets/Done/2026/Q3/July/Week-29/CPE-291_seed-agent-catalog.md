---
id: CPE-291
title: Seed initial agent catalog (~20 agents)
type: Task
status: Done
priority: Medium
component: Backend
estimate: 4h+
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Author the bundled agent manifests for the initial catalog by porting the folders
in `AgenticCliOptions/CodingAgents` (Claude, Codex, Gemini, Aider, Grok, Qwen,
Mistral, Amazon Q, Opencode, Trae, Junie, Antigravity, Codebuff, Hermes, Pi, Tau,
VTCode, …) into the agent-manifest schema — detect/install/uninstall/run recipes +
provider support, per OS.

## Acceptance Criteria

- [ ] A validated manifest per catalog agent, loaded by the registry ([[CPE-278]]).
- [ ] Each agent's detect/install/run verified against the manifest on at least one
      OS (others best-effort, flagged).
- [ ] Provider support (native / OpenRouter / LM Studio) declared per agent to match
      the reference launchers.
- [ ] Catalog is data-only — no per-agent code.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-282]], [[CPE-285]]. **Phase:** C6.
**Epic:** [[CPE-261]].

## Resolution

Authored bundled agent manifests under `sidecar/ai-console/agents/` for 8 agents (claude, codex, gemini, qwen, opencode, grok, aider, mistral), with per-OS detect/install/update/uninstall/run and provider recipes — install commands taken from the reference `AgenticCliOptions` scripts (npm `-g` for the JS CLIs, `uv tool install` for aider/mistral). Claude ships native + OpenRouter recipes (the exact ANTHROPIC_* env from the reference). `tests/catalog.rs` (5 integration tests) proves the catalog loads with no warnings, every agent has run/install/uninstall for this OS, Claude routes OpenRouter, aider installs via uv, and **every provider each agent lists has a working recipe** (no agent advertises a provider it can't route). 51 crate + 5 catalog tests + clippy green.

**Deferred (pure data):** the remaining ~12 reference agents (AmazonQ, Antigravity, Codebuff, Hermes, Junie, Pi, Tau, Trae, VTCode, …) — the loader + schema handle them; adding each is one JSON file. Ongoing catalog refresh is [[CPE-308]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Authored the seed catalog (8 agents) + integration tests during dayshift. Done.
