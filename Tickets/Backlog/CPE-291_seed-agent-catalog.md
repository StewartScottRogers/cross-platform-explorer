---
id: CPE-291
title: Seed initial agent catalog (~20 agents)
type: Task
status: Open
priority: Medium
component: Backend
estimate: 4h+
created: 2026-07-13
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

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
