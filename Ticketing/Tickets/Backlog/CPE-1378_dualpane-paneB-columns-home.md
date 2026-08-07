---
id: CPE-1378
title: "Dual-pane: custom metadata columns and Home-screen actions are unusable/inert in pane B"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (pane-B parity audit, gaps 7/8)

In `src/App.svelte`'s pane-B `<ExplorerPane>` block:

7. **Custom metadata columns** — no `bind:columnWidths`, `activeMetaColumns`, `on:resizeMetaColumns`,
   `on:openColumnPicker` for pane B → column resize/picker unusable there.
8. **Home-screen actions inert** — `inHome` not passed (defaults false, so Home layout logic never fires in
   pane B), and `on:homeSelect`/`on:unpin`/`on:unfavorite`/`on:removeRecent(Folder)`/`on:clearRecents`/
   `on:loadShared`/`on:addNetworkLocation`/`on:removeNetworkLocation` are all unwired.

## Fix direction

Give pane B its own column-width state + wire resize/picker events; pass an `inHome` computed for pane B's
path and wire the same Home-action handlers (most call shared top-level functions — low risk). Add vitest
coverage: simulate a pane-B column resize and assert persisted width state; navigate pane B to Home, dispatch
e.g. `unpin`, assert the shared `pins` store updates. **Shares the pane-B block — serialize with
CPE-1371/1376/1377.**
