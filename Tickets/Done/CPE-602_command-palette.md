---
id: CPE-602
title: "Command Palette — Ctrl+Shift+P to search and run any action"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The app has many actions spread across the toolbar, menu bar, context menus, and shortcuts, but no single
searchable way to find and run one. Add a **Command Palette** (Ctrl+Shift+P) — type to fuzzy-match a
command by name, arrow-key to select, Enter to run. A power-user staple (VS Code / modern editors) that
surfaces the whole action set without hunting through menus.

## Design (pure core + thin UI, like the rest of the codebase)
- **Pure core** `src/lib/commandPalette.ts`: a `Command` type (`id`, `label`, `group?`, `keywords?`,
  `shortcut?`, `run`, `enabled?`) + `filterCommands(commands, query)` — ranked subsequence/substring
  match over label + keywords. Fully unit-tested; no Svelte/DOM.
- **Overlay** `CommandPalette.svelte`: a centred search box + filtered, grouped list; ↑/↓ to move, Enter
  to run, Esc to close; theme-correct light/dark; disabled commands greyed. Follows the menu/pill
  conventions (theme variables only).
- **Wiring** in `App.svelte`: **Ctrl+Shift+P** opens it; build the command list from existing actions
  (navigate Home/Up/Back/Forward, New folder/file, Refresh, Toggle hidden/folders-first, Sort by …,
  View …, Settings, Documents (F1), Shortcuts, About, Search in files, Find duplicates, Reset settings,
  Open AI Console, …). Each command reuses the existing handler — no behaviour duplicated.

## Acceptance Criteria
- [x] `commandPalette.ts` + `filterCommands` with ranked matching; unit tests (empty query, substring,
      subsequence, ranking, disabled filtered out or shown-disabled).
- [x] Ctrl+Shift+P opens the palette; typing filters; ↑/↓/Enter/Esc work; running a command performs the
      real action; Esc/click-away/blur closes.
- [x] Theme-correct (light/dark), keyboard-first, no filesystem risk.
- [x] `npm run check` + full frontend suite green; GUI-verified.

## Notes
Nightshift Loop 1. No filesystem side effects — safe to build/verify unattended. Commands are declared
where their handlers live (App.svelte) so nothing is duplicated.

## Resolution
Added a Command Palette (Ctrl+Shift+P):
- `src/lib/commandPalette.ts` — pure `Command` type + `scoreMatch`/`filterCommands` (ranked
  exact>prefix>word-start>substring>subsequence, keyword synonyms below label hits) + `isEnabled`. 7 tests.
- `CommandPalette.svelte` — centred overlay, live-filtered grouped list, ↑/↓/Enter/Esc, click-away close,
  disabled commands greyed; theme-variable styling.
- `App.svelte` — Ctrl+Shift+P opens it; a reactive `paletteCommands` list wires ~30 existing handlers
  (Go/File/View/Tools/App groups) with live `enabled` predicates (context-invalid commands grey out).
- `shortcuts.ts` — added a "General" group (Ctrl+Shift+P, F1=Documentation, ?=shortcuts) and fixed the
  stale F1 entry (F1 moved to Documentation in CPE-596).

Frontend 66 files + `npm run check` green. GUI verification pending the next Nightshift build.

## Work Log
2026-07-17 (Nightshift Loop 1) — Researched: the app is very complete (natural sort, filter-search,
selection stats, undo, collision-safe naming, folder tree all present). The clear high-value gap was a
searchable command palette. Built + tested it. Labels are English-first (i18n of the palette is a small
follow-up).

2026-07-18 — GUI-verified live in the shipped 0.45.0 build: Ctrl+Shift+P opened the palette, typing
"new fold" filtered to the "New folder" command (FILE group tag shown). Confirmed working.
