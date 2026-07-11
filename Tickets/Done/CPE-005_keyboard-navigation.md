---
id: CPE-005
title: Add keyboard navigation to the file list
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-10
closed: 2026-07-11
---

## Summary

The listing was mouse-only. Add keyboard support: arrow keys to move a selection, Enter to open,
Backspace to go up.

## Acceptance Criteria

- [x] Up/Down arrows move a visible selection highlight through the list
- [x] Enter opens the selected entry (folder navigates; file opens via CPE-006)
- [x] Backspace navigates to the parent directory
- [x] Selection is preserved/reset sensibly when the directory changes
- [x] Focus and selection styles are visible and accessible

## Resolution

Added a `selected` index (-1 = no selection) and a `<svelte:window on:keydown>` handler:
ArrowDown/ArrowUp move the selection (clamped to the list bounds), Enter opens the selected entry,
Backspace goes up. The handler ignores keystrokes originating from `INPUT`/`TEXTAREA` so it won't
hijack typing if a text field is added later. `selected` resets to -1 in `load()`, so the selection
never points at a stale row after the directory changes. Clicking a row also sets the selection, so
mouse and keyboard stay in sync. A reactive statement calls `scrollIntoView({ block: "nearest" })`
on the selected row, so arrowing past the fold scrolls it into view.

Enter integrates with CPE-006: it calls the same `open()`, so folders navigate and files open in the
OS default app.

**A11y fix.** `svelte-check` flagged that the clickable row `<div>` had no keyboard handler. Rather
than suppress it, added an `on:keydown` to the row handling Enter/Space. That created a double-open
risk: a focused row would trigger both its own handler and the window handler (the event bubbles).
Guarded the window handler's Enter branch with `if (target?.closest(".entry")) return;` so the row's
handler is the sole path when a row has focus, while arrows and Backspace still work globally. Also
added a `:focus-visible` outline alongside the `.selected` highlight.

Verified: `npm run check` -> 0 errors, 0 warnings; `npm test` -> 17 passed.

Files changed: `src/App.svelte`, `src/app.css`.

## Work Log

2026-07-11 — Picked up alongside CPE-004. Estimate: 1h.
2026-07-11 — Added `selected` index, window keydown handler (arrows/Enter/Backspace), selection reset in load(), click-to-select, scrollIntoView on selection.
2026-07-11 — svelte-check raised an a11y warning: clickable div lacked a keyboard handler. Added on:keydown to the row.
2026-07-11 — Spotted that this would double-fire Enter (row handler + bubbling window handler both open the entry). Guarded the window handler to bail when a row has focus.
2026-07-11 — svelte-check 0 errors/0 warnings; vitest 17/17. Closing as Done.

## Notes

Selection intentionally resets rather than persists across directory changes — preserving an index
across a different listing would highlight an unrelated file.
