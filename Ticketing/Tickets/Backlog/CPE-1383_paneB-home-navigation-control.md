---
id: CPE-1383
title: "Dual-pane: no UI control routes pane B to the Home screen (Sidebar Home always targets pane A)"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (CPE-1378 follow-up)

CPE-1378 wired pane B's Home-screen actions (Unpin/Unfavorite/recents/network/loadShared) and they're
tested, but there is currently **no way from the live UI to navigate pane B to Home** — the Sidebar's Home
button always calls the pane-A `goHome`/`loadPath(HOME)` path. So the pane-B Home wiring, while correct and
tested, is unreachable by a user today.

## Fix direction

Give pane B a way to reach Home when it's the active pane: route the Sidebar Home button (and/or a Home
breadcrumb/keyboard shortcut) through `activePane` so it navigates pane B to `HOME` (via `navigateB(HOME)`,
which after CPE-1377 short-circuits correctly) when pane B is active, else pane A as today. Touches
`src/App.svelte` (Home-nav handler + Sidebar wiring). Add a test: with pane B active, the Home control
navigates pane B to Home and renders `<HomeView>` there, leaving pane A untouched.
