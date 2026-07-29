---
id: CPE-057
title: Type-ahead find — jump to items by typing their name
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Every desktop file manager lets you type a few letters to jump to the matching item. The app has no
such behaviour — typing does nothing in the list. Add type-ahead: a printable key (no modifier) selects
the next item whose name starts with the typed prefix; keys typed in quick succession accumulate into a
longer prefix; repeating the same letter cycles through matches.

## Acceptance Criteria

- [ ] A printable key with no Ctrl/Alt/Meta selects the next matching item (case-insensitive, prefix)
- [ ] Keystrokes within ~700ms accumulate into a longer prefix; after the gap the buffer resets
- [ ] Repeating a single letter cycles to the next match (wrapping)
- [ ] The matched row scrolls into view (reuses the existing lead-scroll reactive)
- [ ] Does not fire while typing in an input/search/rename, and never hijacks existing shortcuts
- [ ] Pure matcher in `src/lib/typeahead.ts`, unit-tested; App integration test; check + suite green

## Resolution

Added `firstMatchIndex()` in `src/lib/typeahead.ts` (cyclic, case-insensitive prefix match; advance-past
-lead for single chars to cycle, match-at-lead for refinement). `handleKeydown` accumulates a prefix
buffer (700ms window) on unmodified printable keys and `selectOnly()`s the match, reusing the existing
lead-scroll reactive. 7 matcher unit tests + an App integration test (press "b" → beta.png row gains
`selected`). Confirmed working via a DOM dump during development. `npm run check` 0 errors; suite 145
passed; `vite build` clean. Committed, merged to `main`, pushed.

Test note: selecting an item also renders its name in the DetailsPane, so the integration test scopes
its assertion to the list `.row` rather than a bare `getByText`.

## Work Log

2026-07-11 — Nightshift loop: classic file-manager feature with a pure, testable core. Confirmed selection.lead + the App reactive at ~line 290 auto-scrolls the lead into view, and that handleKeydown already bails for INPUT/TEXTAREA and rename mode.

## Notes

Matcher is cyclic: single-char buffers advance past the current lead (cycle); multi-char buffers match
from the current lead (refine). Related: [[CPE-052]] (search) is substring/glob; this is prefix jump.
