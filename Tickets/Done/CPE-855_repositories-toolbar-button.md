---
id: CPE-855
title: Add a Repositories button to the Application toolbar (with AI Console + Agent Board)
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
Per the user directive "keep all the out-of-process applications on the Application Toolbar": add a
**Repositories** button to the main toolbar next to **AI Console** and **Agent Board**, opening the
Repositories browser (`showRepos`). Repositories is a sidecar-platform feature, so — like the AI Console
button — it is shown only when the sidecar platform is active (`aiConsoleAvailable`), grouped with the AI
Console button. Reuses the existing `sidebar.repositories` i18n label (no new key).

## Acceptance Criteria
- [x] A **Repositories** toolbar button sits with AI Console + Agent Board (order: Agent Board · Repositories · AI Console).
- [x] Clicking it opens the Repositories browser; gated on the sidecar platform (`aiConsoleAvailable`), matching the AI Console button.
- [x] Styled to match the other toolbar app buttons; reuses `sidebar.repositories` (no new i18n key).
- [x] `npm run check` clean; tests pass.

## Work Log
- 2026-07-21 — Added the Repositories toolbar button in the sidecar-gated group, next to AI Console; opens showRepos.

## Resolution
`src/App.svelte` — added a **Repositories** toolbar button inside the `{#if aiConsoleAvailable}` group,
before the AI Console button (so the order is Agent Board · Repositories · AI Console — all out-of-process
apps together). `Icon name="code"` (matching the Sidebar's Repositories entry), `on:click` → `showRepos =
true` (the RepoBrowser), label/tooltip reuse the existing `sidebar.repositories` key. Styling: added
`.tb-repos` to the shared `.tb-console, .tb-board, .tb-repos` toolbar-action rule. `npm run check` → 0/0;
App feature tests 26 pass.
