---
id: CPE-468
title: "Data-driven reseller-provider recipe (openai- & anthropic-compatible)"
type: Feature
status: Open
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-467
---

## Summary
A single parameterized launch recipe **per protocol** (openai-compatible, anthropic-compatible) that
any agent can target against any reseller, driven by a reseller **descriptor** (base_url, auth style,
env-var mapping, protocol). Adding a reseller becomes data, not a per-agent recipe. Extends the
routing engine (CPE-285); `openrouter` becomes the first descriptor of the anthropic-compatible shape.

## Acceptance Criteria
- [ ] A reseller descriptor `{ id, name, protocol, base_url, auth_env, model_prefix? }` drives launch.
- [ ] Two protocol templates: openai-compatible (`OPENAI_BASE_URL`/`OPENAI_API_KEY`) and
      anthropic-compatible (`ANTHROPIC_BASE_URL`/key), each fills `{model}`/`{api_key}`/`{base_url}`.
- [ ] An agent gains a reseller provider WITHOUT editing its manifest recipes (the shared template
      applies when the agent declares protocol compatibility).
- [ ] Existing `openrouter` launch behaviour is unchanged (regression test).
- [ ] Unit tests for both templates; clippy clean both feature modes.

## Notes
Foundation for the epic. The current per-agent `provider_recipes` stays supported for overrides.
