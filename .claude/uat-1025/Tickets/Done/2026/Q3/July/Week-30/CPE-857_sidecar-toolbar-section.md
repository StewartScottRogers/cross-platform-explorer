---
id: CPE-857
title: Group the app (sidecar) buttons into their own toolbar section
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
Per the user: the out-of-process app buttons (Agent Deck, Agent Board, Repositories) should live in their
**own section** of the Application toolbar, visually separated from other toolbar buttons, so future
toolbar buttons stay distinct from the apps. Wrap them in a labelled `role="group"` container with a
section divider.

## Acceptance Criteria
- [x] The Agent Board / Repositories / Agent Deck buttons are wrapped in a dedicated `.tb-sidecar-group`
      (`role="group"`), delimited by a divider so they read as one section.
- [x] Buttons behave exactly as before; the group is future-proof (new non-app buttons stay outside it).
- [x] `npm run check` clean; tests pass.

## Work Log
- 2026-07-21 — Wrapped the app buttons in a `.tb-sidecar-group` section with a leading divider.

## Resolution
Wrapped the three out-of-process app buttons (Agent Board, Repositories, Agent Deck) in `App.svelte`'s
Application-toolbar `actions` slot in a single `.tb-sidecar-group` container (`role="group"
aria-label="Apps"`), styled with a leading divider (`border-left` + padding). The apps now read as one
delimited section; any future non-app toolbar button added to the slot stays visibly separate. Buttons,
gating (`aiConsoleAvailable`), and handlers are unchanged.

**Files:** `src/App.svelte` (markup wrap + `.tb-sidecar-group` CSS).
**Verify:** `npm run check` 0/0; full suite 902 passed.
