---
id: CPE-1551
title: "handleKeydown: Ctrl+Shift+F (contentSearch) is dead code — shadowed by the earlier unguarded Ctrl+F (searchFolder)"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-10
---
## Why (found by the independent reviewer of PR #770 / CPE-1547 — pre-existing, NOT that PR's bug)
In `src/App.svelte`'s `handleKeydown`, the `Ctrl+F` branch (~line 5010: `ctrl && key === "f"`) has **no
`!shiftKey` guard** and is checked **before** the `Ctrl+Shift+F` branch (~line 5014). Because a `Ctrl+Shift+F`
press also satisfies `ctrl && key === "f"`, the earlier branch fires first and `return`s — so the
`Ctrl+Shift+F` → **contentSearch** action is **dead code, permanently shadowed** by `Ctrl+F` → searchFolder.

This is unlike the other correctly-ordered modifier pairs in the same function (Ctrl+Shift+D-before-Ctrl+D,
Ctrl+Shift+C-before-Ctrl+C, Ctrl+Shift+T/P, Alt+Enter-before-Enter), which put the more-specific (Shift)
variant FIRST. The Ctrl+F pair is ordered backwards + missing the shift guard.

Net effect: the user's advertised **Ctrl+Shift+F "content search"** shortcut (it's in the cheat-sheet,
`shortcuts.ts`) never triggers — pressing it runs the plain folder search instead.

## Scope (small, frontend-only)
- In `handleKeydown`, either (a) add `&& !event.shiftKey` to the `Ctrl+F` branch, OR (b) move the
  `Ctrl+Shift+F` branch BEFORE the `Ctrl+F` branch — matching the established Shift-variant-first convention
  used by the D/C/T/P pairs in the same function. Prefer whichever matches the surrounding style.
- Verify the fix against the existing keydown test harness (there are tests for the D/C ordering — mirror them:
  add a test that Ctrl+Shift+F triggers contentSearch and plain Ctrl+F still triggers searchFolder).

## Verify
- Unit/jsdom test: Ctrl+Shift+F → contentSearch fires (not searchFolder); plain Ctrl+F → searchFolder still
  fires. `npm run check` + vitest green.

## Notes
Surfaced during CPE-1547's review (keymap registry accuracy check). The keymap registry (CPE-1547) correctly
transcribes shortcuts.ts, so once this handleKeydown ordering is fixed, Ctrl+Shift+F will match its registered
default. Good small batched-run ticket. Relates to the hotkey-customization epic CPE-1484 (the eventual
handleKeydown → keymap migration would also want this fixed).
