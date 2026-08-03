---
id: CPE-1276
title: "AI file copilot UI: instruction → plan preview → confirm → execute → undo (+ model settings)"
type: feature
component: frontend
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-977
---

## Summary
Slice 2 of the AI file copilot (CPE-977). The safe backend is merged (CPE-1275): commands `copilot_plan(root,
instruction)`, `copilot_execute(root, plan)`, `copilot_set_key`/`has_key`/`test`, config `{enabled, base_url,
model}` + key in keychain. Build the UI: the human-in-the-loop preview/confirm/execute/undo surface, plus a
Settings section for the model endpoint mirroring CPE-1273's embedder settings.

## Build
- A copilot surface (a dialog/panel; prefer the app's existing dialog/panel conventions + inline-instant style).
  Flow: user types a natural-language instruction for the CURRENT folder → call `commands.copilotPlan(root,
  instruction)` (busy-cursor invoke) → render the returned **plan preview**: the `op_plan` summary (per-kind counts:
  N moves, N renames, N deletes, N mkdirs, N copies) + the ordered op list (from→to per op), and any validation
  violations (if the plan was rejected, show why, no Confirm). 
- A clear **Confirm** action (the human-in-the-loop gate) → `commands.copilotExecute(root, plan)` → show per-op
  results (succeeded/failed/skipped) + an **Undo** action that reverts via the returned checkpoint handle (wire a
  revert command — reuse the existing checkpoint/restore command if present; if none is exposed to the frontend,
  surface the checkpoint id + a "Revert" that calls it). Deletes went to trash — mention recoverability.
- Empty/needs-config state: if copilot is disabled/unconfigured, prompt to set it up (link to Settings), don't error.
- **Settings** — an "AI file copilot" section mirroring the CPE-1273 "AI content search" section: enable toggle,
  endpoint URL, model name, API key (write-only → keychain via `copilot_set_key`; never echo), Test connection
  (`copilot_test`). Honest copy: works with any OpenAI-compatible chat endpoint — LM Studio local (no key) or OpenAI + key.
- Entry point: a command-palette entry ("AI copilot…" / "Ask the copilot to organize…") and/or a toolbar affordance,
  consistent with how content-search is surfaced.
- Safety UX: make the preview unmistakable that these ops will run on the user's files; Confirm is explicit; show the
  scope (the folder). Never auto-execute.

## Acceptance criteria
- Type instruction → see a real plan preview (counts + op list) → Confirm → execute → per-op results → Undo works.
- Rejected/invalid plan shows violations, no Confirm. Disabled/unconfigured → setup prompt, not an error.
- Component unit tests (jsdom, backend mocked): plan→preview render, confirm→execute call, undo call, violations
  state, needs-config state, no auto-execute without confirm. `npm run check` clean; vitest green; CPE-1271 guard green.
- Docs (CPE-579): a copilot doc page/section + sectionDocs entry if a new section; guard green.

## Notes
Backend is safe (whitelisted plan, root-confined incl. symlink-safe, checkpoint+trash+undo, re-validate on execute).
The UI must preserve the human-in-the-loop contract: nothing executes without an explicit Confirm on the shown plan.
Attended: real plan quality needs the user's LM Studio/OpenAI endpoint.
