---
id: CPE-332
title: "Populate default models for the remaining agent manifests (from reference)"
type: Task
status: Done
priority: Medium
component: Backend
tags: [resource-blocked, needs-reference]
estimate: 1-2h
created: 2026-07-13
closed: 2026-07-14
---

## Summary

CPE-328 added per-provider default-model support and populated `claude.json`. Do the same
for the other bundled agents using the reference's per-agent defaults (e.g. Codex native
`gpt-5.5` / openrouter `openai/gpt-5.1-codex`; read each `<Agent>--openrouter.cmd` /
`--is-installed.cmd` in AgenticCliOptions). Also mirror the reference's per-agent PROVIDER
SUPPORT: Gemini/Grok/AmazonQ/Codebuff have no openrouter — our manifests should not offer a
provider the agent can't use.

## Acceptance
- [x] Every bundled agent launches its providers with sensible default models (only secrets
  required from the user), and only advertises providers it actually supports.

## Work Log

2026-07-14 — Picked up. Estimate: 1-2h. Blocker (`needs-reference`) cleared: the
`AgenticCliOptions` reference is present locally at `Z:\repos\AgenticCliOptions`. Plan:
transcribe each agent's per-provider defaults from its `<Agent>--run.cmd` /
`--openrouter.cmd` / `--local-lmstudio.cmd`, mirror provider SUPPORT (only advertise a
provider the reference actually implements), and give each listed provider a working recipe
with a `defaults.model`.

2026-07-14 — Read the manifest schema (`agents.rs`), the routing engine (`routing.rs`), and
the launch composer (`scope::build_launch`: final args = `run.args` + recipe args + extra).
Confirmed the contract test `every_listed_provider_has_a_working_recipe` requires that any
advertised provider composes — so I only add a provider where its recipe is expressible in
the env/args-template schema.

2026-07-14 — Extracted the reference defaults for all 12 agents:
  - openrouter SUPPORT (has `--openrouter.cmd`): aider, codex, mistral, opencode, pi, qwen,
    tau, vtcode. NO openrouter: codebuff, gemini, grok (matches the ticket's note).
  - native default models: codex `gpt-5.5`, gemini `gemini-2.5-pro`, opencode
    `anthropic/claude-sonnet-4-5`, pi `anthropic/claude-sonnet-4.5`, tau `claude-sonnet-4-6`,
    vtcode `qwen/qwen3-coder`, aider `openrouter/anthropic/claude-sonnet-4.5`.
  - openrouter default models: codex `openai/gpt-5.1-codex`, mistral `mistralai/devstral-medium`,
    opencode/pi/tau `anthropic/claude-sonnet-4.5`, qwen/vtcode `qwen/qwen3-coder`,
    aider `openrouter/anthropic/claude-sonnet-4.5`.
  - lmstudio default id everywhere: `qwen3-coder-30b`.

2026-07-14 — Scope decisions, recorded so the deferrals are honest:
  - **Auto-approve flags omitted.** The reference launchers pass `--yolo` /
    `--dangerously-bypass-approvals-and-sandbox` for unattended personal use. Baking those
    into shipped manifests would auto-approve every command/edit on launch — a security
    regression (cf. CPE-304 threat model) and inconsistent with the conservative bundled
    `claude.json`. Manifests carry only `--model` / `--provider` / provider-config args; the
    user still approves inside the agent's own TUI.
  - **Providers omitted because the recipe schema (env + args templates) cannot generate a
    config FILE**, which these reference launchers require:
      · mistral openrouter — writes a per-run `VIBE_HOME/config.toml`.
      · opencode lmstudio-local — writes a temp `OPENCODE_CONFIG` JSON.
      · tau lmstudio-local — runs a separate `tau setup` upsert before launch.
    These stay unadvertised (honest: we never offer a provider we can't route). Wiring them
    needs a schema extension (a config-file recipe step) — filed as a follow-up thought, not
    this ticket.
  - codebuff / grok left unchanged: platform/subscription-managed, native-only, no model
    selection — already correct.

2026-07-14 — Implemented across `sidecar/ai-console/agents/*.json` (data only, no Rust
change). Verified with `cargo test -p ai-console`: full suite **128 passed / 0 failed** plus
the bundled-catalog contract **7 passed / 0 failed**, including
`every_listed_provider_has_a_working_recipe` — which proves every newly-advertised provider
composes a real launch, and `recipe_defaults_fill_unsupplied_placeholders` (routing) proves
the `defaults.model` fallback means only a secret is required from the user. No frontend/TS
touched, so `npm run check` is N/A.

## Resolution

Populated per-provider default models and honest provider support across the ten bundled
agent manifests (claude was already done in CPE-328), transcribing each agent's real launch
recipe from the `AgenticCliOptions` reference (`<Agent>--run.cmd` / `--openrouter.cmd` /
`--local-lmstudio.cmd`).

**Files changed** (`sidecar/ai-console/agents/`):
- `aider.json` — native + openrouter + lmstudio-local; default `openrouter/anthropic/claude-sonnet-4.5`.
- `codex.json` — native + openrouter (`-c` provider overrides) + lmstudio-local; native `gpt-5.5`, openrouter `openai/gpt-5.1-codex`.
- `gemini.json` — native `--model` with default `gemini-2.5-pro` (no openrouter/lmstudio — unsupported).
- `opencode.json` — native + openrouter (`openrouter/{model}` slug); default `anthropic/claude-sonnet-4-5`.
- `pi.json` — native + openrouter + lmstudio-local; default `anthropic/claude-sonnet-4.5`.
- `qwen.json` — added openrouter (OpenAI-compatible → OpenRouter `/v1`) and an lmstudio default model; default `qwen/qwen3-coder`.
- `tau.json` — native (`--provider anthropic`) + openrouter; default `claude-sonnet-4-6`.
- `vtcode.json` — native + openrouter + lmstudio-local; relocated the `chat` subcommand out of `run.args` into each recipe so `--model` precedes `chat` (the launcher concatenates `run.args` **then** recipe args); default `qwen/qwen3-coder`.

**Not changed (deliberate, documented above):** `mistral.json`, `codebuff.json`, `grok.json`.

**Tradeoffs:** (1) Auto-approve flags (`--yolo` / bypass) intentionally omitted for safety.
(2) Providers whose reference launcher writes a config *file* (mistral openrouter, opencode &
tau lmstudio) are left unadvertised until the recipe schema gains a config-file step — better
to under-offer than advertise a provider we can't route (the catalog contract test enforces
this). (3) LM Studio key value normalised to `lm-studio` to match the existing
`claude.json`/`qwen.json` convention rather than the reference's `lmstudio`.
