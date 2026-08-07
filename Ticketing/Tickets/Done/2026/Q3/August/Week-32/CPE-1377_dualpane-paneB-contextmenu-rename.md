---
id: CPE-1377
title: "Dual-pane: right-click context menu is inert in pane B and inline rename can't complete there"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (pane-B parity audit, gaps 2/3)

In `src/App.svelte`'s pane-B `<ExplorerPane>` block (vs pane A ~L5005–5012):

2. **Context menu inert** — no `on:rowContext` / `on:driveContext` / `on:contextEmpty` /
   `on:homeItemContext` are wired for pane B, so right-clicking a pane-B row does nothing.
3. **Inline rename can't complete** — `renamingPath`/`renameValue` aren't bound and `on:commitRename`
   isn't wired for pane B, so F2 (once CPE-1370 routes it to the active pane) opens no committable editor.

## Fix direction

Wire the same context-menu handler set for pane B, routed **pane-aware** (dovetails with CPE-1370's
`activePane` routing of destructive keys / `handleContextAction`). Give pane B its own
`renamingPathB`/`renameValueB`, bind them, and route `commitRename` to the correct pane's path. Add vitest
coverage (precedent: `App.contextmenu.test.ts`) — dispatch a synthetic `rowContext` on the pane-B instance
and assert the ctx/action target is the pane-B row; trigger rename on a pane-B row and assert commit hits
the correct path. **Depends on CPE-1370 (active-pane routing) and shares the pane-B block — serialize.**
