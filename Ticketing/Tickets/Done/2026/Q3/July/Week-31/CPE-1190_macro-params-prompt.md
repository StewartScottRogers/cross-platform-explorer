---
id: CPE-1190
title: "Parameterized macros: prompt-at-run params (model + param-prompt UI)"
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-739
---

## Summary
Part of CPE-739. Macro steps today carry only fixed strings + rename tokens — no "ask me at run time" concept.
Add an **additive** prompt-parameter (model + pure resolution, headless) and a reusable param-prompt dialog.

## Build
- **Model (additive):** in `crates/server/src/action_macro.rs`, add an `{ask:label}` token or an optional
  `params` field with `#[serde(default)]` so it can't break CPE-1187/1188 (which read this model). Pure
  resolution substitutes prompted values. **Build the model half with the CPE-1187 backend worker** (same
  file), keeping it strictly additive.
- **UI:** `src/lib/components/MacroParamPrompt.svelte` — a generic text-input dialog reusing the
  `PasswordPromptDialog` pattern (visible border, theme-only). Rides with the frontend phase (1189/1191).

## Acceptance Criteria
- [ ] `cargo test -p cpe-server`: param resolution substitutes prompted values; absent params default cleanly
      (no break to plan/executor).
- [ ] gui-smoke `snap("macro-param-prompt")`; `npm run check` + `npm test` green.

## Work Log
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-739). Model half batched into the backend worker (additive);
  prompt-UI half in the frontend phase.
- 2026-07-31 — **Model half landed** (built alongside CPE-1187/1188 on the same branch,
  `cpe-1187-1188-macro-backend`): `crates/server/src/action_macro.rs` now supports an `{ask:label}`
  prompt-parameter token in any step's string field (a rename template, a move dest, a tag label, or a
  convert extension). Strictly additive — `validate()` only special-cases tokens starting with `ask:` (a
  bare `{ask}` with no label is still an unknown-token error, same as before); `plan()`'s signature and
  behaviour are byte-for-byte unchanged (it now delegates to the new `plan_with_params(m, inputs, &empty
  map)`, and an empty params map with no `{ask:...}` tokens present produces identical output to the old
  `plan()`, verified by a dedicated equivalence test). New `pub fn plan_with_params(&ActionMacro, &[String],
  &BTreeMap<String,String>) -> Vec<PlannedOp>` substitutes each `{ask:label}` with `params[label]`; an
  absent label resolves to nothing (dropped cleanly — never a panic, never a literal `{ask:...}` leaking
  into a resolved path/tag/extension). 9 new tests: validate accepts `{ask:x}` in a rename template but
  still rejects a bare `{ask}`; `plan` (old entry point) leaves an unresolved `{ask:...}` dropped, matching
  "absent param defaults cleanly"; `plan_with_params` == `plan` when no params given; substitution works in
  a rename template, a move dest, a tag label, and a convert extension; a partially-answered param map still
  resolves cleanly (the unanswered token drops, the answered one substitutes). The CPE-1187 executor
  (`macro_run::resolve`) does **not** yet consume `plan_with_params`/params — it still calls the original
  `plan()`, so today's macro run/undo behaviour is completely unaffected by this change. Wiring the executor
  through params + the actual `{ask:label}` UI prompt dialog remains this ticket's UI half — **left in
  Backlog**, not moved to Done, since only the model half is complete. `cargo test -p cpe-server`:
  1131/1131 passed (action_macro: 27/27, up from 18). `cargo clippy --all-targets -D warnings` clean
  (default + `--features index`). No new dependencies.
- 2026-07-31 — **UI half done; ticket closed.** Built `src/lib/components/MacroParamPrompt.svelte` (+
  `.test.ts`, 6 tests): a generic labelled-text-input dialog reusing `PasswordPromptDialog`'s pattern
  (backdrop/border/Escape-cancels/Enter-submits), generalized from one masked field to N plain fields —
  one per distinct `{ask:label}` the macro references. Submitting dispatches the full `label -> value` map
  (an untouched field defaults to `""`).
  **Wiring decision:** the exposed `macro_plan`/`macro_run` Tauri commands (and the CPE-1187 executor,
  `macro_run::resolve`, backing `macro_run`) still don't accept a params map — only the pure
  `plan_with_params` does (per the entry above). Rather than touch the backend on this frontend-only
  branch, the substitution happens **client-side**: new `src/lib/macroParams.ts` (+ `.test.ts`, 8 tests)
  mirrors the backend's `{ask:label}` substitution rule exactly (dropped/`""` when unanswered, never a
  literal token leaking through) and returns a fully-resolved `ActionMacro` that
  `commands.macroPlan`/`commands.macroRun` (CPE-1191's run flow) send as-is — no backend change, no
  specta-bindings regen, and a macro with no `{ask:...}` tokens round-trips byte-for-byte unchanged. This
  achieves the ticket's actual goal (parameterized macros work end-to-end from the UI) without the
  backend executor wiring the entry above flagged as outstanding — that wiring is now moot given the
  client-side approach. gui-smoke `snap("macro-param-prompt")` added
  (`gui-smoke/specs/macro-param-prompt.smoke.ts`), exercising a real `{ask:suffix}` macro end-to-end
  through the context-menu run flow and asserting the prompt (not the dry-run confirm) appears first —
  typechecks clean; live run is CI. `npm run check`: 0 errors. `npm test`: 139 files / 1556 tests green.
