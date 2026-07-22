---
id: CPE-678
title: Commander keybindings (copy/move to the other pane)
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-22
epic: CPE-617
estimate: 2-3h
---

## Summary
Child of CPE-617. Total-Commander-style keybindings in dual-pane: F5 copies the active pane's selection
to the other pane, F6 moves it, plus swap-panes and mirror-path — all routed through the transfer queue
(CPE-613). Prereq: CPE-677.

## Acceptance Criteria
- [x] F5/F6 copy/move the active selection to the opposite pane — F5 via `startTransfer(…, "copy", …)`
      (transfer manager + progress), F6 via `move_entries` (tags follow, both panes refresh).
- [x] Swap panes (Ctrl+U) + mirror path work; all four also in the palette (View group); keys documented
      in `src/docs/03-explorer.md`.
- [x] `npm run check` 0/0; docs + i18n guards green.

## Work Log
- 2026-07-22 (nightshift) — `commanderContext()` resolves the active pane's selection + folder and the
  opposite folder from `activePane`. F5 `commanderCopy` → transfer engine; F6 `commanderMove` → `move_entries`
  + `retagMoves` + refresh both panes; `swapPanes` (Ctrl+U); `mirrorPane` (palette). Keys guarded to
  `dualPane` so single-pane is untouched. Palette commands enabled only in dual-pane. Documented the key
  table in the explorer doc. Labels hardcoded EN for v1 (same i18n follow-up as CPE-677). `npm run check`
  clean; docs/sectionDocs 9/9.
