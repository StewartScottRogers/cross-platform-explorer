---
id: CPE-1548
title: "Hotkeys: Settings → Keyboard shortcuts viewer dialog (read-only, searchable)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1484
created: 2026-08-10
---
## Context
CPE-1547 landed `src/lib/keymap.ts`'s `ACTIONS`/`Keymap`/`loadKeymap` as inert plumbing — nothing reads
it yet. This ticket ships the epic's first user-visible surface: a place to **see** every action's
current binding in one list (the first half of the epic's Goal — "view … every keyboard shortcut").
Read-only in this ticket; CPE-1549 turns the same dialog into the rebind surface. It deliberately mirrors
`src/lib/components/ShortcutsDialog.svelte`'s existing backdrop/dialog/Escape/click-away structure and
column layout (same visual language as the "?" cheat sheet users already know) but reads from
`keymap.ts`'s live `ACTIONS`/`Keymap` instead of the static `SHORTCUT_GROUPS` — it does not replace or
edit `ShortcutsDialog.svelte`, which stays exactly as-is as the quick "?" reference.

## Scope
- New file `src/lib/components/KeyboardBindingsDialog.svelte`: same backdrop/dialog/`Escape`-to-close/
  click-away-to-close pattern as `ShortcutsDialog.svelte` (`role="dialog"` `aria-modal`, visible
  `--dialog-border`, `Icon name="keyboard"`). A text filter input at the top (case-insensitive substring
  match against an action's `description` or `group`). Below it, `ACTIONS` grouped by `.group` (same
  section/h3 layout as `ShortcutsDialog`), each row showing the action's description plus its current
  chord via `chordFor(keymap, id)` in a `kbd`-styled pill (render "Unbound" when the chord is `""`). Takes
  a `keymap: Keymap` prop; the caller loads it once via `loadKeymap()` at open time. No rebind/reset
  controls in this ticket — CPE-1549 extends this same file to add them onto each row.
- `src/lib/components/SettingsDialog.svelte`: insert ONE new self-contained section between the existing
  "Appearance" section (ends ~line 160) and "Native metadata bridge" (~line 162): a
  `<div class="section-title">Keyboard shortcuts</div>` plus one button, "Customize shortcuts…", that
  toggles a local `showKeyboardDialog` boolean and conditionally mounts
  `{#if showKeyboardDialog}<KeyboardBindingsDialog keymap={loadKeymap()} on:close={() =>
  (showKeyboardDialog = false)} />{/if}`. Entirely local state inside `SettingsDialog.svelte` — no new
  prop, no new `createEventDispatcher` event, no `App.svelte` wiring, matching the fact that no other
  Settings sub-dialog is orchestrated by `App.svelte` today either.
- `src/lib/sectionDocs.ts`: add `"keyboard-shortcuts"` to the `Section` union (one line, next to
  `"appearance"` at line 34) and `"keyboard-shortcuts": "36-keyboard-shortcuts"` to `SECTION_DOC` (one
  line at the end of the map, ~line 125), same shape as the adjacent `appearance` entry.
- New `src/docs/36-keyboard-shortcuts.md`: short doc page — what the dialog shows, how to open it
  (Settings → Keyboard shortcuts → Customize shortcuts…), and a forward note that rebinding lands in a
  follow-up (CPE-579 convention).

## How
New `KeyboardBindingsDialog.test.ts` (same jsdom/testing-library harness as `ShortcutsDialog`/
`MacrosDialog` tests): renders every `ACTIONS` entry grouped correctly, shows the right chord (including
an "Unbound" case), the filter narrows the visible rows by description and by group, and it closes on
Escape and on backdrop click. Extend `SettingsDialog.test.ts` with a case asserting the new button opens
the dialog (and that it's absent/closed by default). `src/lib/sectionDocs.test.ts` is an existing
exhaustiveness guard that iterates `SECTIONS` automatically — no edit needed there, it just needs to keep
passing (which requires the doc page above to actually exist under the exact slug).

## Verify
`npx vitest run src/lib/components/KeyboardBindingsDialog.test.ts src/lib/components/SettingsDialog.test.ts src/lib/sectionDocs.test.ts`;
`npm run check`. Fully headless (jsdom component rendering, no real Tauri window, no OS keyboard
interaction — this ticket has no capture/rebind logic yet).

## Notes
**Conflict surface:** one new component file + its test + one new docs page, plus two SMALL localized
edits — `SettingsDialog.svelte` (one section-title, one button, one local boolean, one conditional mount:
~10 lines inserted between the existing lines 160-162) and `sectionDocs.ts` (one union member, one map
entry, both one-liners). No `src/App.svelte`, `src/app.css`, `src/lib/settings.ts`, or `src/lib/theme.ts`
touches. **Dispatch order:** after CPE-1547 (imports `ACTIONS`, `Keymap`, `chordFor`, and
`loadKeymap`/`KEYS.keymap` from it).
