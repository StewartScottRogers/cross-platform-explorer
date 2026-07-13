---
id: CPE-285
title: Provider / model routing engine (env recipes)
type: Feature
status: Open
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
---

## Summary

The "any provider, any model" engine. Encodes each (agent, provider) as a
declarative **routing recipe** that composes the correct environment/flags at
launch — native vendor endpoint, OpenRouter (Anthropic-compatible base URL +
auth token + small-model), local/remote LM Studio (per-run settings), etc. Ported
from the `*--openrouter.cmd` / `*--lmstudio.cmd` launchers into data + Rust.

## Acceptance Criteria

- [ ] Routing recipes are declarative (part of the agent manifest / a provider
      registry), not hardcoded per agent.
- [ ] Composing a launch env for (agent × provider × model) produces the right
      vars/flags; pulls secrets from the vault ([[CPE-279]]).
- [ ] Supports native, OpenRouter, LM Studio local + remote out of the box; new
      providers are added as data.
- [ ] Tests: env composition for each provider recipe.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-279]]. **Phase:** C4. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
