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

## Work Log — 2026-08-10

Branch `cpe-1577-1584-command-surfaces`, batched with CPE-1577 (same hot frontend files). PR #797
(https://github.com/StewartScottRogers/cross-platform-explorer/pull/797).

Chose option A from the ticket's fork: made `?` actually open the cheat sheet (matching what
`shortcuts.ts` / the docs already claimed) rather than dropping the claim. Special-cased a bare `?`
in `handleKeydown` immediately ahead of the type-ahead find block (same guard as every other case
in that handler — no INPUT/TEXTAREA/rename/confirm/quick-look context focused), then removed the
now-dead `case "?"` from the switch below and its stale explanatory comment. `shortcuts.ts` needed
no data change (its `"?"` entry was already correct); `36-keyboard-shortcuts.md` needed no change
either. `input-keyboard-reference.md` had two overstatements corrected: "Press `?` at any time"
(now: fires with the file list focused, types literally in a text field) and "`?` always opens the
cheat sheet regardless of focus" (same correction, in the "these are remappable" limits section).

Tests: new `App.shortcutsCheatsheet.test.ts` proves (a) `?` opens the cheat sheet with the file list
focused, (b) it's still blocked while a text field (e.g. the search box) has focus, and (c) ordinary
type-ahead (e.g. pressing `b`) still selects the matching row and does NOT also pop the cheat sheet —
proving the fix didn't widen the special-case past `?` itself.

Verified locally: `npm run check` (0 errors/warnings) and `npx vitest run` (268 files / 3169 tests,
all green). Did not watch CI on the PR — that's the Foreman's pass.
