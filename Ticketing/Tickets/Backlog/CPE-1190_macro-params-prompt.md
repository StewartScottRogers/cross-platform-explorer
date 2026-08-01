---
id: CPE-1190
title: "Parameterized macros: prompt-at-run params (model + param-prompt UI)"
type: feature
component: Multiple
priority: medium
status: Backlog
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
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-739). Model half batched into the backend worker (additive);
  prompt-UI half in the frontend phase.
