---
id: CPE-575
title: "Workbench — remember the embedded-browser URL"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
epic: CPE-505
estimate: 15m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The Workbench's "open the running app in a browser" URL box forgets your dev-server address every time.
Persist it so the URL (e.g. `localhost:5173`) sticks across opens.

## Acceptance Criteria
- [x] The URL input restores its last value on open (localStorage).
- [x] `npm run check` clean; a component test covers restore.

## Resolution
`WorkbenchView` initialises `url` from `localStorage cpe.workbenchUrl` (defensive) and a reactive `$:`
persists it. Behaviour-only (no new text, no i18n). `WorkbenchView.test.ts` +1 (saved URL → input value on
open) + a `beforeEach` localStorage clear to keep the suite isolated. Full suite **615 pass / 63 files**;
`npm run check` 0/0. Epic CPE-505.
