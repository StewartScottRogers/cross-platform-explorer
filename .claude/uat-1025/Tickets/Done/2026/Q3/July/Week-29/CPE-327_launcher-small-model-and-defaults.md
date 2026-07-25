---
id: CPE-327
title: "Launcher: support providers needing small_model; apply default model"
type: Bug
status: Done
closed: 2026-07-13
priority: Medium
component: Multiple
created: 2026-07-13
---

## Summary

Launching with a provider whose recipe references `{small_model}` (e.g. Claude's
`openrouter`, which sets `ANTHROPIC_SMALL_FAST_MODEL`) fails with
`provider requires '{small_model}' but it was not supplied`. Two gaps in the launcher
(CPE-289):

1. The backend `handle_launch` hardcodes `small_model: None`, so the recipe can never be
   satisfied even if the UI sent one.
2. The UI has no small-model field, and a blank model field sends `""` instead of falling
   back to the agent's `default_model`.

## Fix

- Backend: read `smallModel` from the launch request into `LaunchContext.small_model`.
- UI: add an optional "Small/fast model" field; when blank, default it to the main model.
  When the main model field is blank, fall back to the agent's `default_model`.

Result: an `openrouter`-style provider works with just a model + API key (small model
defaults to the main model); `native` (no placeholders) keeps working with no fields.

## Acceptance
- Selecting Claude × openrouter with a model + key launches successfully.
- native still launches with nothing filled in.

## Work Log
2026-07-13 — handle_launch now reads `smallModel` into LaunchContext.small_model (was
hardcoded None); launcher.html adds an optional "Small model" field, defaults it to the
main model when blank, and falls back to the agent's default_model when the model field is
blank. 69 ai-console tests + clippy green. Done.
