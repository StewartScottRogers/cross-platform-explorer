---
id: CPE-330
title: "Wire LM Studio local provider (URL auto-detect) into manifests"
type: Feature
status: Done
priority: Medium
component: Backend
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The reference has a first-class `local-lmstudio` provider per agent: auto-detect the LM
Studio URL (probe loopback + LAN IPv4s on :1234/:1235), `lms load <model>`, query
`/v1/models` and fall back to the actually-loaded model, render a settings template with
the URL injected. We already have `ai-console::lmstudio` code but it is NOT wired into any
manifest recipe or the routing `base_url`. Wire it up: add an `lmstudio-local` provider
recipe (base_url from detection via ProviderDefaults/`{base_url}`), use lmstudio.rs for
detection, and expose it in the launcher's provider list. `_resolve-lmstudio-url.ps1` in
the reference is the detection algorithm to port/confirm against lmstudio.rs.

## Acceptance
- Selecting an agent × lmstudio-local launches against a detected local LM Studio with the
  loaded model, no manual URL entry.

## Work Log

### 2026-07-13 — Wired lmstudio-local end to end (branch CPE-330-lmstudio-wiring)

Wired the existing (previously dead) `ai-console::lmstudio` detection into the routing /
manifest / launcher path so `agent × lmstudio-local` launches with a detected URL and the
endpoint's loaded model — no manual URL entry.

Changes:
- `src/lmstudio.rs`: added `PROVIDER_ID = "lmstudio-local"`; a pure `resolve_launch()`
  that merges detection into the caller's `base_url`/`model` (caller-pinned wins, detection
  fills gaps, passthrough when nothing detected); `candidates(lan)` (loopback first, then
  LAN IPv4) and best-effort `lan_ipv4()` (connected-UDP-socket trick, no packets sent);
  `detect_default()` now probes loopback + LAN. Refactored ports into a `PORTS`/`ports_for`
  helper. Added 5 unit tests (ports_for, candidates ordering, resolve_launch x3).
- `src/console.rs` (`handle_launch`): when provider == `lmstudio-local`, run
  `detect_default()` and merge via `resolve_launch()` before building the LaunchContext.
  Only this provider pays the probe cost.
- `agents/claude.json`: added `lmstudio-local` to providers + a recipe
  (`ANTHROPIC_BASE_URL={base_url}`, `ANTHROPIC_AUTH_TOKEN=lm-studio`,
  `defaults.base_url=http://127.0.0.1:1234`, `--model {model}`).
- `agents/qwen.json`: added an OpenAI-compatible `lmstudio-local` recipe
  (`OPENAI_BASE_URL={base_url}/v1`, `OPENAI_API_KEY=lm-studio`, `OPENAI_MODEL={model}`).
- `src/launcher.html`: the blank-model default no longer forces the vendor default for
  `lmstudio-local` — a blank model is sent blank so the backend adopts the loaded model.
  The provider dropdown is populated from the manifest `providers`, so lmstudio-local now
  appears automatically.
- `tests/catalog.rs`: 2 integration tests (detected URL+model injection; recipe base_url
  fallback when undetected).
- `docs/adding-an-agent.md`: documented the auto-detected `lmstudio-local` provider.

Results: `cargo build` OK; `cargo test` 77 unit + 7 integration pass (0 fail, 2 ignored
manual); `cargo clippy --all-targets` clean.

Assumptions / scope decisions (dayshift, no user to ask):
- Wired the recipe into `claude` (Anthropic-compatible, mirrors the existing openrouter
  recipe — highest confidence) and `qwen` (OpenAI-compatible, standard OPENAI_* vars).
  Left `codex` and the other OpenAI-family agents out because their custom-endpoint
  contract is config-file-based/less certain; the mechanism is pure manifest data, so they
  can copy the qwen recipe once their exact env contract is confirmed. Documented the
  pattern in adding-an-agent.md.
- `lms load <model>` (actively loading a model via the `lms` CLI) is NOT implemented —
  it mutates LM Studio state and needs the `lms` binary. Detection + fall-back-to-loaded
  model satisfies the acceptance ("launches with the loaded model"); active loading is a
  follow-up.
- LAN detection probes loopback + this host's own primary LAN IPv4 (covers "Serve on Local
  Network" bound to the LAN interface). Full-subnet scanning of *other* hosts was left out
  to keep the launch probe fast/predictable per the sidecar tiebreaker.
- If nothing is detected and no model is supplied, the recipe has no default model, so the
  launch fails loudly on `{model}` rather than launching a wrong model — intentional.
