---
id: CPE-1547
title: "Hotkeys: keymap action registry + persisted override store (foundation, inert)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1484
created: 2026-08-10
---
## Context
CPE-1484's goal is "view every keyboard shortcut in one place and rebind it." Today the only source of
truth for shortcuts is `src/lib/shortcuts.ts`'s `SHORTCUT_GROUPS` — a **hand-transcribed, read-only**
list that must be kept in lockstep with the ~35 hardcoded `if` branches in `App.svelte`'s
`handleKeydown` (starts `src/App.svelte:4955`). There is no data model a UI could read/write to show or
change a binding. Separately, `src/lib/macroBindings.ts` already solved chord normalization for the one
existing user-remappable case (per-macro hotkeys): `normalizeHotkey`, `hotkeyFromEvent`, and
`matchHotkey` are exported and directly reusable — this ticket does not re-derive that logic.

This ticket adds the single source of truth for every **built-in** action's default chord plus a
persisted override store. It is **inert on landing**: nothing calls the new APIs yet, and
`handleKeydown` is untouched, so there is zero behavior change and zero risk to the real key-handling
path. Same "inert plumbing first" shape as CPE-1544 in the high-contrast epic. Migrating
`handleKeydown`'s branches to actually consult this store is deliberately **out of scope** here (and for
this whole batch) — the epic brief itself flags that migration as "the bulk of the work," and doing it
now would mean editing the same 7300-line `App.svelte` handler from multiple tickets at once, which is
exactly the hot-shared-file collision this batch is designed to avoid. It becomes its own ticket once
CPE-1548/1549 give the store a UI to be exercised through.

## Scope
- New file `src/lib/keymap.ts`:
  - `export type ActionId = ...` — a string-literal union of every action in `SHORTCUT_GROUPS` that maps
    to one real fixed keybinding (skip "Type a name" — jump-to-item isn't a chord — and the Macros
    group's "(user-configured)" placeholder row, which `macroBindings.ts` already owns). ~29 ids, e.g.
    `"back" | "forward" | "up" | "refresh" | "editAddress" | "searchFolder" | "findFiles" |
    "contentSearch" | "instantSearch" | "openItem" | "newTab" | "closeTab" | "reopenTab" | "nextTab" |
    "prevTab" | "selectAll" | "clearSelection" | "copy" | "cut" | "paste" | "duplicate" | "addToDropStack"
    | "undo" | "rename" | "deleteToTrash" | "deletePermanent" | "newFolder" | "copyAsPath" | "properties"
    | "toggleDetails" | "popOutPreview" | "commandPalette" | "docsHelp" | "shortcutsCheatSheet"`.
  - `export interface ActionDef { id: ActionId; group: string; description: string; defaultChord: string }`
    — `defaultChord` in the canonical form `normalizeHotkey` (imported from `./macroBindings`) produces;
    values transcribed 1:1 from `SHORTCUT_GROUPS`'s `keys` column.
  - `export const ACTIONS: ActionDef[]` — the full registry, grouped in the same order as
    `SHORTCUT_GROUPS` (Navigation/Tabs/Selection/File actions/View/General) for a 1:1 mental model with
    the existing read-only cheat sheet.
  - `export type Keymap = Record<ActionId, string>` (chord per action, `""` = unbound) and
    `defaultKeymap(): Keymap` built from `ACTIONS`.
  - `chordFor(keymap, id)`, `actionForChord(keymap, chord)` (reverse lookup on an already-normalized
    chord), `setChord(keymap, id, rawChord): Keymap` (normalizes via `normalizeHotkey`; immutable —
    returns a new object), `resetChord(keymap, id): Keymap`, `resetAll(): Keymap`.
  - `findConflicts(keymap): { chord: string; ids: ActionId[] }[]` — every chord currently bound to 2+
    actions; empty/unbound chords never conflict.
  - `serializeKeymap(keymap): string` / `parseKeymap(json): Keymap` — tolerant, mirroring
    `macroBindings.ts`'s `serializeBindings`/`parseBindings`: parse a JSON object, keep only entries whose
    key is a known `ActionId` and whose value normalizes to a valid chord or `""`, silently drop
    everything else (unknown/renamed ids, corrupt values, non-object JSON), and **backfill
    `defaultKeymap()` for any action missing from the parsed object** so a partial or stale persisted map
    always yields a complete `Keymap`. Never throws.
- `src/lib/settings.ts`: one `KEYS.keymap: "cpe.keymap"` entry next to `KEYS.macroBindings` (~line 71),
  and `loadKeymap`/`saveKeymap` next to `loadMacroBindings`/`saveMacroBindings` (~lines 512-516), same
  shape: `loadKeymap = (): Keymap => { const v = state[KEYS.keymap]; return v === undefined ?
  defaultKeymap() : parseKeymap(JSON.stringify(v)); }`, `saveKeymap = (v: Keymap): void =>
  write(KEYS.keymap, v);`.
- Nothing else lands in this ticket: no component, no `App.svelte` edit, no docs page (there is no
  user-visible surface yet — CPE-1548 adds the first one).

## How
New `src/lib/keymap.test.ts`: `defaultKeymap()` covers every `ActionId` exactly once; `setChord`/
`resetChord`/`resetAll` immutability (input object unchanged); `findConflicts` with 0/1/2/3-way
collisions and confirming unbound (`""`) chords are exempt; `serializeKeymap`/`parseKeymap` round-trip,
plus tolerant-degrade cases (malformed JSON, unknown action id, invalid chord string, a partial map
missing several actions merges cleanly with defaults). Extend `src/lib/settings.test.ts` with
`loadKeymap`/`saveKeymap` coverage the same way the existing `loadMacroBindings`/`saveMacroBindings`
tests are structured (default-on-missing, round-trip, corrupt-value degrades).

## Verify
`npx vitest run src/lib/keymap.test.ts src/lib/settings.test.ts`; `npm run check`. Fully headless — pure
TS logic, no DOM, no Tauri invoke, no OS keyboard interaction.

## Notes
**Conflict surface:** two new files (`src/lib/keymap.ts`, `src/lib/keymap.test.ts`) plus one small,
localized addition to `src/lib/settings.ts` (one `KEYS` entry ~line 71, one load/save pair ~lines
512-516, shaped exactly like the adjacent `macroBindings` block) and its test file. No
`src/App.svelte`, `src/app.css`, `src/lib/sectionDocs.ts`, or `src/lib/theme.ts` touches. **Dispatch
order:** first — CPE-1548, CPE-1549, and CPE-1550 all import from `keymap.ts` and depend on this landing.
