---
id: CPE-456
title: "Remove 'Pop out preview' from the Application menu"
type: Task
status: Done
priority: Low
component: Frontend
tags: [ready]
estimate: 15m
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Remove the "Pop out preview" entry from the **Application** menu (MenuBar). User asked for it gone.
The pop-out feature itself stays reachable via the preview-pane button, Ctrl+Shift+O, and drag.

## Acceptance Criteria
- [x] The "Pop out preview" item no longer appears under the Application menu.
- [x] Its now-dead menu-action case is removed; the pane button + shortcut + drag still work.
- [x] `npm run check` clean.

## Work Log
2026-07-15 — Picked up. Remove MenuBar item (id pop-out-preview) + the orphaned select-case in App.svelte.

## Resolution
Removed the `pop-out-preview` item from the Application menu (`MenuBar.svelte` `menus`) and its now-orphaned `case "pop-out-preview"` in `App.svelte` `onMenuSelect`. `popOutPreview()` stays — still reachable via the preview-pane pop-out button, Ctrl+Shift+O, and drag-the-tab-bar. `npm run check` clean.
