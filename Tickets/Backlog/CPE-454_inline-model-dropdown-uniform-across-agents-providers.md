---
id: CPE-454
title: "Inline Model dropdown — change on a dime, uniform across agents/providers"
type: Defect
status: Open
priority: High
component: Frontend
tags: [ready]
created: 2026-07-15
epic: CPE-444
---

## Summary
The current Model control is unusable. CPE-449 shipped a **"Browse…" button that opens a modal
overlay** — search, click, close — which is far too much ceremony to change a model. Replace it with
an **inline, always-visible Model dropdown** in the launch bar that lets you switch models **instantly
("on a dime")**, and that **works exactly the same regardless of which agent or provider is selected**.

## User intent (verbatim need)
- A **dropdown with a list of models** right there in the bar — not a modal you have to open.
- **Change on a dime**: pick a different model in one action, no extra clicks/steps.
- **Uniform**: "No matter who the provider is or the Agent, the Model dropdown should work the same."

## Current behaviour (what's wrong)
- Model is a free-text `#model` input plus a **"Browse…"** button (`#model-browse`) that opens the
  `#model-overlay` modal (`openModelBrowser` in `launcher.html`). Selecting a model = open modal →
  wait for fetch → type filter → click row → modal closes. Too slow; not a dropdown.
- Behaviour differs by context: the text field just shows the agent's default as a placeholder; there
  is no consistent, populated dropdown across agents/providers.

## Desired behaviour
- The Model control is an **inline dropdown / editable combobox** in the launch bar, always visible,
  populated with the available models for the current provider/reseller.
- **One-action switching**: open the dropdown (or type-ahead) and pick — the choice applies immediately.
- **Identical UX for every agent + provider**: same control, same interaction, same population logic,
  whether the provider is OpenRouter, a native agent default, LM Studio, or any reseller.
- The list refreshes when the provider/reseller changes (reuse the CPE-449 `/api/models` fetch chain).
- **Never a dead end**: because some models aren't listed (or the list can't load / offline), the
  control stays **editable** (type a custom model id) and honours the agent's default when left alone.
- Handles a long list (OpenRouter ~300) gracefully — **type-ahead filtering inside the dropdown**
  (e.g. an `<input>` + `<datalist>`, or a filtering combobox), not a raw 300-row native `<select>`.

## Acceptance Criteria
- [ ] The "Browse…" modal (`#model-browse` / `#model-overlay` / `openModelBrowser`) is removed in
      favour of an inline dropdown control in the launch bar.
- [ ] The dropdown is populated with the current provider/reseller's models and updates when the
      provider changes; switching models takes a single action.
- [ ] The control behaves identically across all agents and providers (one code path, no per-agent
      special-casing of the interaction).
- [ ] Remains editable/custom-capable and falls back cleanly when the list is empty/offline; the
      agent's default still applies when nothing is chosen.
- [ ] The "Fast model" (`#smallModel`) field gets the same treatment (or is explicitly out of scope
      with a note).
- [ ] jsdom launcher tests cover: population on provider change, one-action pick sets `#model`,
      type-ahead filtering, and the offline/custom fallback. `npm run check` clean.

## Notes
Supersedes the CPE-449 "Browse…" overlay UX (keep its fetch/normalize backend — `/api/models`,
`model_catalog::normalize_models` — and re-present it as an inline dropdown). Reuse the reseller/model
plumbing from CPE-444/447/449; this is a **frontend UX rework**, not new backend. Verify the final
feel in a real GUI run (the "on a dime" quality is a hands-on judgement).
