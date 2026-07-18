---
id: CPE-589
title: "Native provider rejects OpenRouter-format models — normalize to a native alias"
type: Bug
status: Done
priority: High
component: Sidecar
tags: [ready]
epic: CPE-528
created: 2026-07-17
closed: 2026-07-17
---

## Summary
A swarm (and any native launch) failed when the Model was an OpenRouter-format id like
`anthropic/claude-opus-4.8`: the **`native`** provider is the agent's own login (Claude Code), which
wants a bare alias/id and rejects `vendor/model` ("model may not exist"). The shared model picker offers
those `anthropic/…` ids for native, so users pick invalid ones.

## Diagnosis (verified end-to-end)
Reproduced the swarm's exact launch through the real `cmd /c` + MSVC-quoting path (via Node's `spawn`,
identical to `portable_pty`):
- `--model anthropic/claude-opus-4.8` → "issue with the selected model… may not exist".
- `--model claude-sonnet-4-5` → **works**: Claude loaded the swarm host, wrote memory, posted to the
  mailbox (`mailbox.jsonl` created). So the whole stack is sound; only the model value was wrong.

## Fix
`routing::compose_launch` — for `provider == "native"`, normalize the model via `native_model`: an
OpenRouter-style `vendor/model` is reduced to a family alias (`opus`/`sonnet`/`haiku`, which the native
CLI resolves to its latest), else the part after the slash; bare/native ids pass through. Applies to all
native launches, so a model picked from the shared catalog now launches natively.

## Acceptance Criteria
- [x] `native` launch with `anthropic/claude-opus-4.8` → `--model opus`.
- [x] Bare native ids (`claude-sonnet-4-5`) unchanged; other providers unaffected.
- [x] Tests + clippy green.

## Follow-on
Model picker should present native-appropriate models when the provider is `native` (cosmetic — the
launch now normalizes regardless).
