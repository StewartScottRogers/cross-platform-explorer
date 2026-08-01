---
id: CPE-1212
title: "App-wide: centralize hard-coded error/danger colours into a --danger theme var"
type: chore
component: Frontend
priority: low
status: Backlog
tags: ready
created: 2026-08-01
epic: CPE-579
---

## Summary
Recurring reviewer nit across epics (CPE-1189 MacrosDialog `.err` #c0392b/#d05656; CPE-1208 LinkBadge
`.broken` #b5433a; plus pre-existing `FileList.svelte .agent-badge.removed`, `ExplorerPane.svelte`,
`SidecarManager.svelte`, `AgentTimeline.svelte`). Error/danger colours are hard-coded hex literals scattered
across components instead of a single theme variable. Not a MENUS violation (those target popup-menu item
text, which is correctly `var(--text)`), and the app is light-theme-only — so purely a consistency/maintainability
cleanup.

## Build
- Add a `--danger` (and maybe `--warn`) token to the single `:root` palette ([[app-is-light-theme-only]]).
- Replace the scattered hard-coded error/danger hexes with `var(--danger)` across the components above.
- No visual change intended (pick the token value to match the current predominant hex).

## Acceptance Criteria
- [ ] Error/danger colours come from `var(--danger)`; grep shows no stray `#c0392b`/`#b5433a`/`#d05656` in
      component styles; `npm run check` + `npm test` green; no visual regression (Visual Critic spot-check).

## Work Log
- 2026-08-01 — Filed by Foreman (workshift) from repeated reviewer nits (CPE-1189, CPE-1208). App-wide polish,
  like the dialog-border CPE-1193.
