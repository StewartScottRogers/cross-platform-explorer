---
id: CPE-846
title: Add an Agent Board button to the main toolbar, next to AI Console
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-841
estimate: 30m
---

## Summary
Follow-up to CPE-844: put a one-click **Agent Board** button on the main toolbar, immediately to the left
of the **AI Console** button, that opens the Agent Board in its own window (`openAgentBoard()`). Unlike the
AI Console button (gated to sidecar-platform builds via `aiConsoleAvailable`), the board button is always
shown — the board works in every build.

## Acceptance Criteria
- [x] A toolbar action button labelled **Agent Board** sits next to the AI Console button, styled to match.
- [x] Clicking it opens/focuses the standalone Agent Board window (`openAgentBoard()`).
- [x] Always visible (not gated on `aiConsoleAvailable`); the AI Console button still appears (sidecar
      builds) to its right.
- [x] `npm run check` clean; frontend tests pass.

## Resolution
`src/App.svelte` — added an **Agent Board** toolbar action button as the first child of the Toolbar's
`actions` slot (so it sits just left of the AI Console button), always shown, `on:click={() =>
openAgentBoard()}`, `Icon name="documents"` (matching the Sidebar's Agent Board icon), label the literal
"Agent Board" (already treated as a product name in the Sidebar/DOC_SECTIONS — no new i18n key), tooltip
reuses the existing `palette.openAgentBoardWindow` key. Styling shares the `.tb-console` rule (renamed to a
`.tb-console, .tb-board` selector group) so it matches the AI Console button exactly.

Verification: `npm run check` → 0/0; App feature tests 26 pass. The AI Console button (sidecar builds)
still renders to the right, gated as before.

## Work Log
- 2026-07-21 — Added the toolbar button next to AI Console, opening the standalone board window. Reused the
  existing palette i18n key for the tooltip + a literal "Agent Board" label (already a product-name literal
  elsewhere) to avoid the 12-locale coverage gate. check clean; tests green. Closing.
