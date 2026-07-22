---
id: CPE-328
title: "Per-provider default models in recipes (steal from AgenticCliOptions)"
type: Feature
status: Done
priority: High
component: Backend
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The reference (`Z:\repos\AgenticCliOptions`) bakes a default model (and small model) per
agent AND per provider, env-overridable — so OpenRouter launches with just an API key.
Ours required the user to type `{model}` and `{small_model}`, which is the friction hit in
testing (`provider requires '{small_model}'`).

## What landed

- Schema: `ProviderRecipe.defaults { model?, small_model?, base_url? }` (additive).
- Routing: `compose_launch` fills any placeholder the caller didn't supply from the
  recipe defaults; a supplied value always wins; `api_key` never defaults (it's a secret).
- `claude.json`: `default_model: claude-sonnet-4-5`; native recipe passes `--model` with
  default `claude-sonnet-4-5`; openrouter defaults `anthropic/claude-sonnet-4.5` +
  `anthropic/claude-haiku-4.5`, and clears `ANTHROPIC_API_KEY` (so the OpenRouter token
  wins — also stolen from the reference). Test added. 70 ai-console tests green.

Result: Claude × openrouter launches with only an API key. Remaining manifests → CPE-332.
