---
id: CPE-351
title: "Discoverable AI Console entry point (toolbar button)"
type: Bug
status: Done
closed: 2026-07-13
priority: High
component: Frontend
created: 2026-07-13
---

## Summary

The AI Console — a headline feature — has no discoverable entry point. It can only be opened
from Settings (gear) → Sidecar manager → the ai-console row → a generic "Open" button. Users
can't find it ("there is no AI Console button"). Add a top-level **AI Console** button to the
application toolbar, shown only when the sidecar platform is active (`platformActive()`), that
opens the console. It degrades cleanly (hidden) in plain-explorer builds.

## Acceptance
- An obvious "AI Console" button appears in the toolbar in sidecar-platform builds; clicking it
  opens (or focuses) the console. Hidden when the platform isn't available.
- `npm run check` + `npm test` green.

## Work Log
2026-07-13 — User: "There is no AI Console button." Adding a toolbar entry point.

2026-07-13 — Implemented on branch `CPE-351-ai-console-button`.
- `App.svelte`: `aiConsoleAvailable` (set from `platformActive()` on mount); an **AI Console**
  button in the application Toolbar's `actions` slot (via `<svelte:fragment slot="actions">`
  so it can be conditional), calling `openAiConsole()`. Scoped `.tb-console` style next to the
  gear. Hidden in plain-explorer builds (no platform).
- `npm run check` 0 errors; frontend suite 296 pass. (Also reachable as before via Settings →
  Sidecar manager → Open; this just makes it discoverable.)
