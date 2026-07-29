---
id: CPE-355
title: "Copy to… / Move to… folder (native picker) from the context menu"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

Today, relocating files means cut/copy → navigate → paste. Add **Copy to folder…** and
**Move to folder…** to the file context menu: they open a native folder picker (the dialog
plugin the app already bundles) and run the existing `copy_entries` / `move_entries` backend
against the chosen destination — no navigation dance.

## Design (frontend)
- `ContextMenu.svelte`: two items on a selection in a real folder (not Home/archive), gated by
  the existing `canTerminal` flag; dispatch `copy-to` / `move-to`.
- `App.svelte`: `open({ directory: true })` from `@tauri-apps/plugin-dialog` → invoke
  copy/move with `dest`; reuse `reportResults`; reload if the destination is the current folder,
  and (for move) always reload since items leave.

## Assumptions (Nightshift — user asleep, logged per policy)
- Uses the OS folder picker (dialog:default capability is already granted). Multi-select of the
  destination is off (one target). Same-folder / into-self guards are already enforced by the
  backend (`copy_entries`/`move_entries`).

## Acceptance
- With items selected, "Copy to folder…"/"Move to folder…" open a folder dialog; choosing a
  folder copies/moves the selection there; the view refreshes; cancel is a no-op.
- `npm run check` + `npm test` green.

## Work Log
2026-07-13 — Filed during Nightshift (loop 1). Research: the only universally-useful explorer
gap left (favorites/thumbnails/columns/recent-folders already shipped) — cut/paste works but
there's no direct "send to folder". Implementing.

2026-07-13 — Implemented on branch `CPE-355-copy-move-to-folder`.
- `ContextMenu.svelte`: "Copy to folder…" / "Move to folder…" on a selection in a real folder
  (gated by `canTerminal`), dispatching `copy-to` / `move-to`.
- `App.svelte`: `copyMoveToFolder(move)` — `open({directory:true})` from `@tauri-apps/plugin-
  dialog`, then `copy_entries`/`move_entries` to the chosen dest; reuses `reportResults`; a
  move reloads + is pushed to the undo stack; a copy reloads only when dest == current folder.
- `ContextMenu.test.ts` (new): items appear + dispatch in a real folder, hidden otherwise.
- `npm run check` 0 errors; frontend suite 305 pass; `npm run build` ok. The dialog itself
  needs a GUI eyeball; wiring/gating are tested.
