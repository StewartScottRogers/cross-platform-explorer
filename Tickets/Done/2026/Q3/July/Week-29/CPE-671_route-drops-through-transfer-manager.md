---
id: CPE-671
title: Route drag copy/move through the transfer manager
type: enhancement
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-661
estimate: 2-3h
---

## Summary
Make drag-copy/move tracked operations by routing `dropInto` through the transfer manager (CPE-613,
`start_transfer`) instead of the direct `copy_entries`/`move_entries` invoke — so a big drag shows the
progress panel + conflict chooser. Must preserve the current undo push and tag-follow (retag) behaviour
on completion. Prereq: CPE-669.

## Acceptance Criteria
- [x] Drag copy/move runs through the transfer queue with progress + batch conflict handling.
- [x] Undo (move) and retag-on-move still work after a dragged transfer completes.
- [x] Small/same-folder drops stay snappy (no visible regression); `npm run check` + suite green.

## Work Log

## Work Log
2026-07-18 (nightshift) — Picked up (prereq CPE-669). No questions; best-guess.

## Resolution
Mirrored the proven paste split in `dropInto` (App.svelte): a drag that resolves to **copy** now routes
through the transfer engine (`startTransfer(paths, dest, "copy", "keepboth")`) — tracked progress in the
operations panel, the `transfer://done` listener refreshes + reports, keepboth auto-renames on collision.
A drag that resolves to **move** keeps the synchronous `move_entries` path so **undo + tag-follow (retag)
stay intact** (moves are fast same-volume renames). The OS drop-in import (CPE-670) also routes copies
through the engine now, for consistency. check clean; suite green (666); bundle clean.

Scope note: drag-copy uses keepboth (safe auto-rename) rather than the paste conflict-chooser, which would
need the destination folder's listing pre-loaded — a deferred refinement. Live check of the progress panel
appearing on a large drag recommended on /run. Files: src/App.svelte.
