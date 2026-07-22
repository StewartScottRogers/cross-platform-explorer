---
id: CPE-465
title: "Model picker shows only one model — the field's current value pre-filters the whole list"
type: Defect
status: Done
closed: 2026-07-15
priority: High
component: Sidecar (AI Console)
tags: [ready]
created: 2026-07-15
estimate: 30m
epic: CPE-261
---

## Summary
Opening the Model dropdown in the AI Console shows only **one** model (e.g. for OpenRouter), even
though the published catalog snapshot carries 342 OpenRouter models. The user expects to see dozens.

## Root cause
The model field doubles as both the *selected-model display* and the *filter input*. On agent select,
`applyLastUsed()` sets `$("model").value` to the **last-used full model id**
(`launcher.html:538`). When the menu opens, `renderModelMenu()` filters `modelOptions` by
`q = $("model").value.toLowerCase()` (`launcher.html:1102-1104`). Because `q` is a *complete* model
id, it matches exactly one entry — so a returning user always sees a single row. (A brand-new user
with an empty field sees the full list, which is why it looked intermittent.)

The GitHub-hosted list itself is correct and already downloadable/updatable: `models-index.json` in
the `model-catalog` release has 342 OpenRouter models, refreshable via the `model-snapshot` workflow.
The defect is purely the picker's open-time filtering.

## Fix
Separate the committed value from the filter query. Track a `modelQuery` string that is set **only**
when the user types in the field (`input` event); opening the menu (focus / toggle) renders with an
empty query so the full list shows. `renderModelMenu()` filters by `modelQuery`, not by the field's
current value. Picking or reopening resets the query to "".

## Acceptance Criteria
- [x] Opening the Model menu with a last-used model committed shows the **full** reseller list, not one row.
- [x] Typing in the field filters the list live (substring over id / display name).
- [x] Picking a model still commits it to the field and closes the menu.
- [x] Reopening the menu after a pick shows the full list again.
- [x] Launcher jsdom test covers the regression (committed value must not pre-filter on open).

## Resolution
Separated the filter query from the committed field value in `launcher.html`:
- Added a `modelQuery` variable; `renderModelMenu()` now filters by it instead of `$("model").value`.
- `openModelMenu()` resets `modelQuery = ""`, so opening always shows the full reseller list.
- The `input` handler sets `modelQuery` from the field text (typing filters); picking a row and
  reopening reset it to "".

Files: `sidecar/ai-console/src/launcher.html` (query/value split), `src/lib/ai-console-launcher.test.ts`
(new CPE-465 regression: committed id must not pre-filter on open; updated the CPE-460 filter test to
drive filtering via the `input` event). Confirmed the published catalog already carries 342 OpenRouter
models (`models-index.json` in the `model-catalog` release) — no data change needed. `npm run check`
clean; 28 launcher tests pass.

## Work Log
- 2026-07-15 — Picked up. Estimate: 30m. Traced the picker: snapshot data is correct (342 openrouter
  models published), so the defect is client-side.
- 2026-07-15 — Root cause found: `applyLastUsed()` pre-fills the field with the last-used model id and
  `renderModelMenu()` filtered by the field value → exactly one match on open.
- 2026-07-15 — Fixed via `modelQuery` split; added regression test; check + tests green. Closed.

## Notes
Reported by the user: "The Models drop down only has one model for open router … I expect to see
dozens." Sibling of CPE-460 (picker built) and CPE-463 (picker legibility).
