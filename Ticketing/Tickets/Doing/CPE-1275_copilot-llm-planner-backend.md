---
id: CPE-1275
title: "AI file copilot backend: LlmPlanner (OpenAI-compatible) → FileOpPlan + plan/execute+undo commands"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-977
---

## Summary
Slice 1 of the AI file copilot (CPE-977): natural-language → a SAFE, whitelisted file-op plan, previewed +
confirmed by the human, executed with undo. The pure plan model is already built (`op_plan.rs`: closed `FileOp`
set move/rename/delete/mkdir/copy, `validate` root-scoped envelope, `summarize`). This slice adds the LLM seam +
the plan/execute commands. UI is slice 2 (CPE-1276). Mirror the CPE-1273 embedder pattern for the model connection
(LM Studio local, no key / OpenAI + key).

## Build
- **`LlmPlanner` seam + `HttpPlanner`** (cpe-server): OpenAI-compatible **/chat/completions** via the already-present
  `ureq` (reuse, no new dep), behind a feature gate like `pdf-thumb`/`http-embedder`. System prompt instructs the
  model to output ONLY a JSON `FileOpPlan` over the closed op set, given the user instruction + a listing of the
  target folder (names + is_dir). Use the model's JSON/structured-output mode where available; robustly parse
  (extract the JSON object; reject anything that isn't a valid FileOpPlan). NEVER exec free-form — the plan is
  whitelisted by construction. A `FakePlanner` for tests (deterministic, no network).
- **Config**: reuse the CPE-1273 config pattern — persisted `{ enabled, base_url, model }` + API key in the OS
  keychain (SecretAccess seam, service e.g. `cpe.copilot`). Default disabled. Key never plaintext/logged/returned.
- **`copilot_plan(root, instruction) -> {plan, summary, violations}` command**: build the plan via the planner,
  `op_plan::validate` against `PlanLimits::new(root, cap)` (every path must stay under `root`; op-count cap),
  return the validated plan + `summarize` counts for preview. NO filesystem changes here. Errors (model
  unreachable/bad output/validation failures) → clear structured result, never panic.
- **`copilot_execute(root, plan) -> result` command**: RE-VALIDATE the plan (never trust a stale/tampered plan),
  take a checkpoint FIRST for undo (reuse the checkpoint/snapshot engine — snapshot/restore_plan), then apply the
  whitelisted ops using existing fs primitives; **deletes go to TRASH (recoverable), not hard-delete**. Return
  what changed + the checkpoint id for undo. Idempotent/safe on partial failure (report per-op result).
- Register commands + specta bindings (regen bindings.gen.ts); capability entries; async + spawn_blocking.

## Acceptance criteria
- cargo build/test/clippy clean (all feature modes); no new dep (reuse ureq + keyring + op_plan + fs + checkpoint);
  CPE-1271 guard + bindings drift green; disabled path adds nothing to the base build.
- Unit tests (FakePlanner, no network): NL→plan parse, validate rejects out-of-root/over-cap plans, execute applies
  + checkpoints + trashes deletes, re-validate blocks a tampered plan, key never leaks, never panics on bad model output.
- Safety: no path escapes root; no op outside the whitelist; nothing executes without a validated plan; undo works.

## Notes
UI (instruction input → plan preview → confirm → execute → undo) is CPE-1276. Human-always-in-the-loop: execute is
only ever called after the user confirms the previewed plan.
