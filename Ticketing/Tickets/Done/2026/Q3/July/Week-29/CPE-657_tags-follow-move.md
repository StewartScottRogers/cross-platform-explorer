---
id: CPE-657
title: Tags follow an in-app move (not just rename)
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Nightshift loop. CPE-652 made tags follow a rename; they still orphaned on a **move**. A `retagMoves`
helper now re-keys each moved file's tags to its new path after all three move flows (Ctrl+V move, the
"Move to…" picker, and drag-and-drop move).

## Acceptance Criteria
- [x] `retagMoves(moves)` calls `retagPath(from, to)` per successfully-moved file (best-effort, no-op if untagged).
- [x] Wired into all three move completions (paste-move, picker-move, drag-drop-move).
- [x] `npm run check` clean; suite green.

## Work Log
2026-07-18 (nightshift) — Completed the tags-follow story for moves (rename was CPE-652).
