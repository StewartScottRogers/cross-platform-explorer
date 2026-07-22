---
id: CPE-623
title: Operations panel + route copy-paste through the transfer engine
type: feature
component: Frontend
priority: high
status: Done
tags: ready
estimate: 2h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-613
---

## Summary
Children CPE-623 (panel) + the copy slice of CPE-625 (wiring). A bottom-corner **operations panel**
lists active + just-finished transfers with a progress bar, live file counts, and cancel/dismiss;
Ctrl+V **copy**-paste now runs through the async engine (CPE-620) so a large copy shows progress and can
be cancelled. Idle-hidden — nothing renders when no transfer is running.

## Decisions / scope
- **Copies** route through `start_transfer` (policy `keepboth` == the old auto-rename-on-collision, so
  behaviour is preserved); **moves** stay on the existing synchronous `move_entries` path to keep undo
  support + instant same-volume rename. Copies were already non-undoable, so there's no undo coupling.
- On `transfer://done`, App refreshes the current folder and reports the outcome.
- Routing move + the "Copy to…/Move to…" picker through the engine, and a batch conflict dialog
  (overwrite/skip chooser), remain follow-ups (CPE-624/625) — deferred until attended verification since
  move carries undo coupling.

## Acceptance Criteria
- [x] `TransferPanel.svelte` renders `$transfers` with per-transfer progress %, file counts, error
      count, and cancel (running) / dismiss (finished); hidden when empty; theme-variable styling.
- [x] `initTransfers()` + a `transfer://done` listener wired in App (refresh + report); torn down on destroy.
- [x] Ctrl+V copy uses `startTransfer(..., "copy", "keepboth")`; move unchanged.
- [x] `npm run check` clean; full frontend suite green (68 files / 648).
- [x] GUI-verified: copying a folder shows the panel with progress and the files land.

## Work Log
2026-07-18 (dayshift) — Built panel + wired copy-paste; GUI-verified in an installed build.
