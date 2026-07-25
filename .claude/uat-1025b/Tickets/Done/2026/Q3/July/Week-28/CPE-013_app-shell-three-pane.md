---
id: CPE-013
title: App shell — three-pane layout and status bar
type: Feature
status: Done
priority: High
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Restructure the single-pane app into the Explorer shell: navigation sidebar (left), content (centre),
details pane (right), with a status bar along the bottom.

## Acceptance Criteria

- [x] Grid layout: sidebar | content | details pane, with a full-width status bar
- [x] Panes scroll independently; only the content area scrolls the file list
- [x] Details pane can be toggled off
- [x] Layout holds at small window sizes without overlap
- [x] App.svelte decomposed into components under src/lib/components/

## Resolution

`#app` is now a 5-row grid: tab bar / nav toolbar / command bar / main / status bar. `main` is a
grid of `220px 1fr` that becomes `220px 1fr 300px` when the details pane is on (`.with-details`).
Each pane owns its own scrolling (`min-height: 0` on the grid children, which is what actually makes
a nested scroll container work inside a grid row).

Decomposed the monolithic App.svelte into 8 components under `src/lib/components/`: TabBar,
NavToolbar, CommandBar, Sidebar, FileList, HomeView, DetailsPane, StatusBar, plus a shared Icon.
App.svelte is now purely state + orchestration.

## Work Log

2026-07-11 — Picked up. Built the 5-row / 3-column grid shell.
2026-07-11 — Decomposed App.svelte into 8 components + Icon.
2026-07-11 — Details pane toggles the grid template rather than just hiding the element, so the content pane reclaims the space. Closing as Done.

## Notes
