---
id: CPE-1549
title: "Hotkeys: press-to-set remap capture + live conflict warning + reset-to-default"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1484
created: 2026-08-10
---
## Context
CPE-1548 shipped `KeyboardBindingsDialog.svelte` as a read-only viewer over CPE-1547's keymap store. This
ticket turns it into the epic's actual rebind surface — the second half of the Goal ("… and rebind
them, with conflict detection"): a press-to-set capture control per row, a live warning when the newly
captured chord collides with another action (via CPE-1547's `findConflicts`), and reset-to-default (per
row) / reset-all. This still does **not** touch `App.svelte`'s `handleKeydown` — rebinding here only
changes what CPE-1547's persisted `Keymap` reports back; wiring the app's real shortcut dispatch to
actually consult it is the deferred migration work the epic brief calls out as its own bulk of effort,
left for a future ticket once this store has real user-facing traffic through it.

## Scope
- New file `src/lib/components/HotkeyCaptureInput.svelte`: a small, self-contained, reusable control with
  no knowledge of the keymap or conflicts — a button showing the current chord prop (or "Click to
  set…" when empty); clicking arms capture mode, during which its own
  `<svelte:window on:keydown|capture>` listener (active only while armed) calls `event.preventDefault()`
  and `event.stopPropagation()` so the captured key never reaches the app underneath, builds the chord via
  `hotkeyFromEvent` (imported from `../macroBindings`), `Escape` cancels capture without emitting
  anything, and any other keystroke commits and dispatches a `set` event carrying the normalized chord
  (or `""` if `hotkeyFromEvent` rejected it — e.g. a bare letter with no `Ctrl`/`Alt` modifier — in which
  case it stays armed and the parent shows nothing changed).
- `src/lib/components/KeyboardBindingsDialog.svelte` (CPE-1548): per row, swap the read-only chord `kbd`
  for a `HotkeyCaptureInput`; on its `set` event compute `setChord(keymap, id, chord)`, run
  `findConflicts` on the result, and if the new chord now collides with another action show an inline
  warning naming the colliding action with "Rebind anyway" / "Cancel" (choosing "Rebind anyway" leaves
  the other action unbound rather than letting two actions silently share a chord — never a silent double
  binding). Add a per-row "Reset" button (`resetChord`) and a dialog-level "Reset all to defaults" button
  (`resetAll`). Every accepted change calls `saveKeymap` (from `settings.ts`, already exported by
  CPE-1547) immediately — no separate Save/Cancel/Apply, matching the app's existing immediate-persist
  Settings pattern (theme, contrast, and every other Settings control save on change already).

## How
New `HotkeyCaptureInput.test.ts`: simulates arming (click), a keydown while armed producing the right
normalized `set` chord, `Escape` cancelling without a `set`, and a rejected combo (e.g. a bare letter)
staying armed with no `set`. Extend `KeyboardBindingsDialog.test.ts` with: a successful rebind (asserts
`saveKeymap` called with the updated map), a rebind that collides (asserts the warning renders naming the
colliding action, and that "Cancel" leaves both bindings unchanged while "Rebind anyway" unbinds the
other action), a per-row reset, and "Reset all to defaults".

## Verify
`npx vitest run src/lib/components/HotkeyCaptureInput.test.ts src/lib/components/KeyboardBindingsDialog.test.ts`;
`npm run check`. Fully headless — simulated `KeyboardEvent`s dispatched in jsdom via testing-library, no
real OS key injection, no Tauri window.

## Notes
**Conflict surface:** one new file (`HotkeyCaptureInput.svelte` + its test) plus edits confined to
`KeyboardBindingsDialog.svelte` — CPE-1548's new file, not one of the hot shared files — swapping the
per-row display and adding reset/conflict UI. No `src/App.svelte`, `src/lib/settings.ts`,
`src/lib/sectionDocs.ts`, `src/lib/theme.ts`, or `src/app.css` touches. **Dispatch order:** after
CPE-1548 — edits the same dialog file CPE-1548 creates, so run sequentially after it lands, not in
parallel with it.
