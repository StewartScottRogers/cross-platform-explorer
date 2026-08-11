---
id: CPE-1622
title: "Agent Deck: Model picker shows \"Couldn't load models\" for a tool's built-in (native) login"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready]
created: 2026-08-11
---

## Why
Found writing the docs depth pass on `src/docs/04-ai-console.md` (Agent Deck, CPE-1619, epic CPE-1569) —
verifying the Model picker's behavior against the real code before documenting it.

## The gap
Many agent manifests list `"native"` as a provider option (the tool's own built-in login — e.g. Claude
Code's own `claude login`, no API key needed). `populateModels()` in
`sidecar/ai-console/src/launcher.html:1948-1949` builds the models request straight from the selected
provider:
```js
const reseller = ($("provider").value || "").trim() || "openrouter";
...
const d = await api("/api/models?reseller=" + encodeURIComponent(reseller));
```
So picking `native` sends `reseller=native`. The backend, `handle_models`
(`sidecar/ai-console/src/console.rs:990-1033`), special-cases only the local LM Studio provider
(`reseller == crate::lmstudio::PROVIDER_ID`); everything else is looked up against the verified snapshot
by reseller name, and `native` isn't a reseller the snapshot (or the live per-reseller fetch fallback)
covers — so the request errors, and the frontend's model dropdown renders the **error state**
(`renderModelMenu`, `launcher.html:1973-1978`): *"Couldn't load models."* with a **Refresh** button that
retries the same doomed request.

The Model **field** itself still works fine as free text (or blank, which falls back to the agent's
`defaultModel`) — this is purely the dropdown's model **list** failing. But a user selecting a tool's
plain built-in login (arguably the simplest, most common setup — no key required) is greeted with an
alarming error and a Refresh button that can never succeed, instead of the dropdown simply not offering a
model list for a provider that doesn't have one.

## Fix
Skip the model-list fetch entirely for `native` (and any other non-reseller, non-local provider) the same
way LM Studio is special-cased — either hide/disable the dropdown affordance with a neutral message (e.g.
"This provider doesn't offer a model list — type a model id, or leave it blank for the default."), or
short-circuit `populateModels()` client-side before calling `/api/models` at all. Whichever end fixes it,
add a case so `native` (and future non-reseller providers) never hit the reseller live-fetch path.

**Conflict surface:** `sidecar/ai-console/src/launcher.html` (`populateModels`/`renderModelMenu`),
possibly `sidecar/ai-console/src/console.rs` (`handle_models`) if the fix is server-side.

## Acceptance criteria
- Selecting a `native`-only provider never shows "Couldn't load models." — either no dropdown fetch
  happens, or it shows an accurate "no model list for this provider" message instead of an error.
- A regression test (or manual note in the ticket if the harness can't reach the sidecar HTTP layer)
  covers the native-provider case.

## Notes
Low priority: cosmetic/confusing, not destructive — the Model field still works as free text regardless.
Small, self-contained. Model: sonnet or haiku.
