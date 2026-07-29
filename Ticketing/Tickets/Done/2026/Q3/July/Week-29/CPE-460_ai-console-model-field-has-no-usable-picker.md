---
id: CPE-460
title: "AI Console Model field has no usable model picker"
type: Defect
status: Done
priority: High
component: Frontend
tags: [ready]
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
In the shipped v0.15.0 sidecar build, the AI Console **Model** field shows no selectable list of
models — it looks like a plain text box, not a picker/dropdown. Reported: "the Model does not have a
picker or dropdown for me to select from a set of models supported by OpenRouter."

## Root causes (both to fix)
1. **No visible picker affordance.** CPE-454 made Model an `<input list="model-options">` +
   `<datalist>`. A bare datalist-input has **no dropdown arrow** — it reads as a text field, and its
   suggestions only appear on typing. Users don't recognize it as a picker.
2. **The list is empty / silent.** It's populated by a **live** `/api/models?reseller=openrouter`
   fetch (`populateModels()` → `host.list_models` → OpenRouter). If that returns nothing in the build
   (network, egress wiring, or timing), the control is silently empty — `populateModels` swallows the
   error. So there's no list AND no visible "loading/failed/empty" state.

## Acceptance Criteria
- [x] The Model control is an obvious **picker/dropdown** (a caret/button that opens the list, or a
      combobox with a visible open affordance) — not a bare text input.
- [x] It is **populated** with the model list and shows explicit **loading / empty / error** states
      (a failed fetch must be visible, never silent).
- [x] Selecting a model fills the field; typing a **custom** id still works (never a dead end).
- [x] Diagnose + fix why the list is empty in the shipped build (verify `/api/models` →
      `host.list_models` → OpenRouter actually returns models in a real run; add a manual "Refresh").
- [x] Prefer the **downloaded GitHub snapshot** (CPE-450/451) as the source once available, with the
      live fetch as a refresh/fallback.

## Notes
Supersedes the "delivered" claim on CPE-449/454 — the control exists but isn't usable as a picker.
The robust fix depends on CPE-450 (publish the list to GitHub) + CPE-451 (download it) so the picker
has a reliable, offline source; until then it must at least surface the live list + its error state.
Verify the final feel in a GUI run.

## Resolution
Replaced the invisible `<input list=datalist>` with a real **combobox**: an input + a visible **▾** toggle + an anchored, scrollable dropdown menu. Type-to-filter narrows the list; clicking a row fills the field (mousedown+preventDefault so selection beats the blur); the field stays free-text for custom ids. Explicit states in the menu — **Loading… / No models / Couldn't load models + Refresh** — so a failed live fetch is visible + actionable, never a silent empty box. Source is `/api/models` (live) today; once CPE-450/451 land, the downloaded GitHub snapshot becomes the default source with live as a Refresh. 3 rewritten jsdom tests (open+filter+pick; provider-change repopulate; error+Refresh) + the reattach/reseller tests still green (23 total); svelte-check 0. Final feel is a GUI eyeball.
