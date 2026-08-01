---
id: CPE-1207
title: "GUI: New Link… dialog + creation wiring (symlink/hardlink)"
type: feature
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
Part of CPE-715 (after CPE-1206). A "New Link…" dialog to create symlinks/hardlinks.

## Build
- "New Link…" entry in the empty-area context menu + command palette; a small dialog: kind (Symlink | Hardlink),
  target field with a native Browse picker ([[path-inputs-need-picker]]), link-name field. On confirm call
  `commands.createSymlink`/`createHardLink`, reload + inline-rename like `createNewItem` (`App.svelte`).
- Surface the backend's Windows Developer-Mode/elevation error via `showNotice` (do NOT swallow it; no elevation
  modal — [[avoid-modal-permission-popups]]).

## Acceptance Criteria
- [ ] gui-smoke: render pin of the dialog; a headless click-through creating a **hardlink** (unprivileged-safe)
      in a temp dir that then lists. `npm run check` + `npm test` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). Batch with CPE-1209 (both edit App.svelte/ContextMenu).
