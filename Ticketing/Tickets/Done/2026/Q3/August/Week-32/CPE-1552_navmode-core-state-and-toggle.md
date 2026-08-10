---
id: CPE-1552
title: "Navigation Mode: core mode-state reducer + off-by-default Settings toggle (inert, foundation)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1487
created: 2026-08-10
---
## Context
CPE-1487's goal is an opt-in vim-modal layer ("Navigation Mode") over the file list: `h/j/k/l` move,
`gg`/`G` jump, `v` visual-range select, `d`/`y`/`p` cut/copy/paste, `/`/`:` start filter/mini-command —
strictly off by default so PURPOSE.md's fast/small/predictable core is untouched when the setting is off.

This ticket lands the foundation: a pure, DOM-free mode-state reducer plus the Settings toggle that will
gate the whole feature. It is **inert on landing** — nothing calls the reducer from `App.svelte` yet, and
`handleKeydown` (`src/App.svelte:4955`) is untouched, so there is zero behavior change and zero risk to
real key handling. Same "inert plumbing first" shape as CPE-1547 (keymap.ts) in the hotkey-customization
batch this epic depends on.

**Scope note on remapping:** the epic brief mentions modal bindings eventually being remappable through
CPE-1484's `src/lib/keymap.ts` registry. This ticket does NOT extend `keymap.ts`'s `ActionId` union —
doing so now would require a matching edit in every ticket of this batch that needs a chord, which is
exactly the shared-file collision this decomposition avoids. V1 ships a small, hardcoded internal chord
table inside the new file below; wiring it into the full remappable keymap store is an explicit,
separately-ticketed fast-follow once this lands and has a UI to exercise it through.

## Scope
- New file `src/lib/navMode.ts`:
  - `export type NavMode = "normal" | "visual"`.
  - `export interface NavState { mode: NavMode; pendingChord: string; pendingCount: string }` —
    `pendingChord` buffers multi-key sequences (`g` waiting for a second `g`); `pendingCount` buffers a
    typed numeric prefix (`"3"` before `j` means "3 down"), vim-style.
  - `export function initialNavState(): NavState` — `{ mode: "normal", pendingChord: "", pendingCount: "" }`.
  - `export type NavIntent =`
    `| { kind: "none" }`
    `| { kind: "motion"; dir: "left" | "down" | "up" | "right" | "top" | "bottom"; count: number }`
    `| { kind: "enterVisual" } | { kind: "exitVisual" }`
    `| { kind: "op"; op: "delete" | "yank" | "paste" }`
    `| { kind: "startFilter" } | { kind: "startCommand" }`.
  - `export function reduceNavKey(state: NavState, key: string): { state: NavState; intent: NavIntent }` —
    pure reducer over a single already-normalized `KeyboardEvent.key` value (no chord modifiers; Navigation
    Mode's V1 grammar is unmodified single keys only, matching the vim TUIs surveyed). Handles: digits
    `1`-`9` (and `0` only mid-count) accumulate into `pendingCount`; `h`/`j`/`k`/`l` emit a `motion` intent
    consuming any pending count (default 1) and reset `pendingCount`; `g` alone buffers into
    `pendingChord` awaiting a second key, a following `g` emits `motion top`, any other key clears the
    buffer and re-dispatches that key from a clean state; `G` emits `motion bottom`; `v` toggles
    `mode` between `"normal"`/`"visual"` emitting `enterVisual`/`exitVisual`; `Escape` forces `mode` back
    to `"normal"` and clears both buffers, emitting `exitVisual` only if it was visual (else `none`);
    `d`/`y`/`p` emit `op` with `delete`/`yank`/`paste` respectively and do not change mode; `/` and `:`
    emit `startFilter`/`startCommand` and do not change mode (the consumer owns what "starting" means);
    any unrecognized key clears pending buffers and emits `{ kind: "none" }` (never throws, never blocks
    unrelated keys from being seen as unhandled by a caller).
  - No imports from `App.svelte`, `selection.ts`, or `commandPalette.ts` — this file only knows about mode
    state and key-to-intent mapping, nothing about what an intent *does*.
- `src/lib/settings.ts`: one `KEYS.navigationModeEnabled: "cpe.navigationModeEnabled"` entry next to
  `KEYS.nativeBridgeEnabled` (~line 82), and `loadNavigationModeEnabled`/`saveNavigationModeEnabled` next
  to `loadNativeBridgeEnabled`/`saveNativeBridgeEnabled` (~lines 337-338), identical shape: `export const
  loadNavigationModeEnabled = (): boolean => read(KEYS.navigationModeEnabled, false, isBool);` /
  `export const saveNavigationModeEnabled = (v: boolean) => write(KEYS.navigationModeEnabled, v);`
  (default `false` — off).
- `src/lib/components/SettingsDialog.svelte`: one checkbox row, "Navigation Mode (experimental — vim-style
  keyboard layer)", following the `nativeBridgeEnabled` row exactly (local `let navigationModeEnabled =
  settings.loadNavigationModeEnabled();` near line 88, `on:change` calling
  `settings.saveNavigationModeEnabled` near line 90, checkbox markup near line 186). Off by default, no
  other UI change.
- Nothing else lands in this ticket: no `App.svelte` edit, no docs page (CPE-1555 adds the doc), no
  `keymap.ts` edit.

## How
New `src/lib/navMode.test.ts` (pure, no DOM, mirrors `src/lib/keymap.test.ts`'s style): every direction
key from a fresh state; count-prefixed motion (`"3"` then `"j"` → `{kind:"motion",dir:"down",count:3}`,
and count resets after); `gg` sequence (first `g` → `none` + `pendingChord:"g"`, second `g` → `motion top`
+ buffer cleared); a `g` followed by an unrelated key clears the buffer and does not swallow the second
key incorrectly; `G` → `motion bottom`; `v` toggles mode both directions; `Escape` from visual → `normal`
+ `exitVisual`, `Escape` from normal → `none` (mode already normal); `d`/`y`/`p` → correct `op` intents
without changing `mode`; `/`/`:` → `startFilter`/`startCommand`; an unrecognized key (e.g. `"q"`) → `none`
and clears any pending buffers. Extend `src/lib/settings.test.ts` with
`loadNavigationModeEnabled`/`saveNavigationModeEnabled` coverage shaped like the existing
`loadNativeBridgeEnabled`/`saveNativeBridgeEnabled` tests (default-false, round-trip). Extend
`src/lib/components/SettingsDialog.test.ts` with one assertion that the new checkbox renders unchecked by
default and calls `saveNavigationModeEnabled(true)` on toggle, mirroring the existing
`nativeBridgeEnabled` checkbox test.

## Verify
`npx vitest run src/lib/navMode.test.ts src/lib/settings.test.ts src/lib/components/SettingsDialog.test.ts`;
`npm run check`. Fully headless — pure TS logic plus one jsdom component test, no Tauri invoke, no real
OS keyboard interaction, and the feature is unreachable from the running app until CPE-1556 wires it in.

## Notes
**Conflict surface:** two new files (`src/lib/navMode.ts`, `src/lib/navMode.test.ts`) plus small, additive,
localized touches to `src/lib/settings.ts` (one `KEYS` entry + one load/save pair, shaped exactly like the
adjacent `nativeBridgeEnabled` block) and `src/lib/components/SettingsDialog.svelte` (one checkbox row) and
their test files. No `src/App.svelte`, `src/lib/keymap.ts`, `src/app.css`, or `src/lib/sectionDocs.ts`
touches. **Dispatch order:** first — CPE-1553, CPE-1554, CPE-1555, and CPE-1556 all import `NavState`/
`NavIntent`/`NavMode` from `navMode.ts` and depend on this landing.
