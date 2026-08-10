---
id: CPE-1584
title: "Bug: `?` doesn't open the shortcuts cheat sheet — shadowed by type-ahead jump"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-10
closed:
---

## Why / found by
Surfaced during CPE-1582 UAT (docs). In `App.svelte`'s `handleKeydown`, a bare `?` is consumed by the
type-ahead jump-to-item block (`event.key.length === 1`, ~line 5188) **before** it can reach the
`case "?": shortcutsOpen = true` branch (~line 5213). So pressing `?` never opens the shortcuts cheat sheet —
a pre-existing defect (shipped in #780/CPE-1557; the code even carries a comment admitting it). The claim that
`?` opens the cheat sheet appears in `src/lib/shortcuts.ts`, `src/docs/36-keyboard-shortcuts.md`, and the new
`src/docs/input-keyboard-reference.md` (all faithfully inherited from the source of truth).

## Fix (scope when picked up)
Decide the intended behavior and make code + docs agree:
- EITHER make `?` (Shift+/) open the cheat sheet — special-case it ahead of the type-ahead block (careful: it's a
  printable char, so only when no text-entry context is focused), matching what the docs/cheat-sheet claim; OR
- drop the `?` claim from `shortcuts.ts` + both doc pages and expose the cheat sheet only via the toolbar/`F1`/palette.
- Whichever: keep `shortcuts.ts`, `36-keyboard-shortcuts.md`, and `input-keyboard-reference.md` consistent with the code.

## Notes
Low priority (cheat sheet is reachable via the toolbar "?"/help). Verify the exact type-ahead vs switch ordering in
`App.svelte` `handleKeydown` before changing. Model: sonnet.
