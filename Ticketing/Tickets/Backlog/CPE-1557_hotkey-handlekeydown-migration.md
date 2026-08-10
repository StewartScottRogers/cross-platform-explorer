---
id: CPE-1557
title: "Wire handleKeydown to the persisted keymap so remaps actually change key behavior"
type: Task
status: Backlog
priority: High
component: Frontend
epic: CPE-1484
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1484 (hotkey customization) shipped the whole data model and UI: `keymap.ts` (registry +
`actionForChord`/`chordFromEvent`/`defaultKeymap`), `settings.ts` `loadKeymap()`/`saveKeymap()`
(persisted under `cpe.keymap`), and `KeyboardBindingsDialog`/`SettingsDialog` to view & remap.
**But it is all still INERT for behavior**: `App.svelte`'s `handleKeydown` uses hard-coded
`if (ctrl && key === 'f')`-style branches and never consults the persisted keymap, so a user's
remap changes what the dialog *shows* but not what the keys *do*. This ticket closes that gap —
the deferred migration noted at CPE-1484 close.

## Goal
`App.svelte`'s `handleKeydown` resolves remappable built-in actions through the **effective
keymap** (`loadKeymap()` → `actionForChord(keymap, chordFromEvent(event))`), so a user override
saved via the bindings UI immediately changes key behavior. **Zero behavior change when there are
no overrides** — with the default keymap, every shortcut fires exactly as it does today.

## Acceptance criteria
- A reactive keymap source in `App.svelte`: load once via `settings.loadKeymap()`; re-read/refresh
  when the bindings are saved (the SettingsDialog/KeyboardBindingsDialog save path) so a remap takes
  effect without an app restart.
- `handleKeydown` routes the **chord-based remappable actions** in the `keymap.ts` `ActionId`
  registry through `actionForChord(keymap, chordFromEvent(event))` instead of hard-coded chord
  literals. Non-remappable / contextual keys (navigation-mode motions, type-ahead jump, Space
  preview, context-sensitive Enter/Escape, pane routing) are OUT of scope — leave them exactly as-is.
- **Provably no regression at defaults:** a test asserting that with `defaultKeymap()` every migrated
  action resolves to the same handler/branch it hits today (mirror the inert-first proof pattern used
  for the keyboard-nav epic). Guard against double-firing (a chord must not trigger both the old
  literal branch and the new keymap path).
- Conflict/empty-chord safety: an action whose chord a user cleared (`""`) simply doesn't fire; a
  user-assigned chord that now collides with a built-in resolves deterministically (first match, as
  `actionForChord` already defines).
- `npm run check` clean; `npm run test` (vitest) green including the new no-regression test; clippy
  N/A (frontend-only).
- Docs: no new user-facing *section* (the hotkey docs page already exists from CPE-1484) — update it
  only if the "remaps now take effect" behavior needs a line. No `sectionDocs.ts` change expected.

## Notes / guardrails
- **`App.svelte` is a ~7300-line hot file.** Slice carefully; make the smallest surgical change that
  routes the migrated branches through the keymap. Follow the inert-first / opt-in-safe discipline
  from the keyboard-nav epic — prove default = zero behavior change with a test before anything else.
- Reuse `chordFromEvent` (permissive, matches bare-key defaults like F5/F2/Delete) — do NOT use the
  strict `normalizeHotkey` on live events.
- Do not broaden scope to non-chord actions or navigation mode.
- Model: opus (hot-file integration, matches the CPE-1556 App.svelte lesson).
