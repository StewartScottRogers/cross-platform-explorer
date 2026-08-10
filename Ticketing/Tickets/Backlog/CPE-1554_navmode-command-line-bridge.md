---
id: CPE-1554
title: "Navigation Mode: ':' mini command-line bridging into existing Command Palette verbs"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1487
created: 2026-08-10
---
## Context
CPE-1487's brief calls for a `:`-style mini command line inside Navigation Mode that bridges into the
**existing** Command Palette verbs rather than inventing a second command registry. `src/lib/
commandPalette.ts` already owns matching: `interface Command { id, label, group?, keywords?, shortcut?,
run, enabled? }`, `scoreMatch(text, query): number`, `filterCommands(commands: Command[], query: string):
ScoredCommand[]`, `isEnabled(c: Command): boolean`. The palette's `Command[]` list itself is assembled in
`App.svelte`'s reactive `$: paletteCommands = [...]` (starts ~line 1031) and rendered through
`src/lib/components/CommandPalette.svelte`.

This ticket builds a small, self-contained mini command-line UI that reuses `filterCommands` against
whatever `Command[]` it's handed — it does not fork the matching logic, and it does not touch
`commandPalette.ts`, `CommandPalette.svelte`, or `App.svelte`'s `paletteCommands` assembly. Wiring it to
actually receive the real `paletteCommands` array and mount inside the app is CPE-1556's job (the single
ticket that touches `App.svelte`); this ticket ships the reusable piece and proves it against a mock
`Command[]` in tests.

## Scope
- New file `src/lib/navCommandLine.ts`:
  - `export function filterNavCommands(commands: Command[], query: string): Command[]` — a thin wrapper
    around `filterCommands`/`isEnabled` (both imported from `./commandPalette`, no edits to that file)
    that strips a leading `:` from `query` if present, filters out `!isEnabled(c)` commands, and returns
    the plain `Command[]` in score order (unwrapping `ScoredCommand`). Empty/whitespace-only query (after
    stripping `:`) returns the full enabled command list unfiltered, so opening the command line with no
    input shows every available verb.
- New file `src/lib/components/NavCommandLine.svelte`:
  - Props: `commands: Command[]` (the candidate list to filter, passed in by the caller — not sourced
    internally).
  - A single-line text input styled as a bottom-of-pane status bar prompt (`:` prefix glyph, monospace),
    plus a short live-filtered result list below it using `filterNavCommands`.
  - Dispatches a Svelte `run` custom event with the selected `Command` on Enter/click; dispatches a
    `cancel` event on `Escape`. Does not itself call `command.run()` — the caller (CPE-1556) decides when
    to invoke it, matching how `CommandPalette.svelte` already delegates execution to its parent.
  - Colors via existing semantic tokens only (`var(--text)`, `var(--text-dim)`, `var(--bg-elevated)` /
    whatever `CommandPalette.svelte` already uses for its own list-item styling — read that component for
    the reference pattern, do not edit it). No new CSS custom properties, so no `[data-theme=...]` block
    edits are needed in `src/app.css`.
- No `App.svelte`, `commandPalette.ts`, or `CommandPalette.svelte` edits.

## How
New `src/lib/navCommandLine.test.ts` (pure, no DOM): `filterNavCommands` strips a leading `:`; empty
query returns all enabled commands unchanged in score order; a query matching a subset returns only
those, disabled commands (`enabled: () => false`) are excluded even on an exact-label match; unwraps
`ScoredCommand` back to plain `Command`. New `src/lib/components/NavCommandLine.test.ts` (jsdom, mirrors
the existing `src/lib/components/CommandPalette.test.ts` if one exists, else follows the harness pattern
used by `src/lib/components/CommandBar.test.ts`): renders with a mock `commands` prop, typing filters the
visible list, Enter on the highlighted item fires a `run` event with that `Command`, `Escape` fires
`cancel` without a `run` event.

## Verify
`npx vitest run src/lib/navCommandLine.test.ts src/lib/components/NavCommandLine.test.ts`; `npm run
check`. Fully headless — pure filtering logic plus one jsdom component test against a mock `Command[]`,
no Tauri invoke, unreachable from the running app until CPE-1556 mounts it.

## Notes
**Conflict surface:** three new files (`src/lib/navCommandLine.ts` + test, `src/lib/components/
NavCommandLine.svelte` + test). Reads `src/lib/commandPalette.ts` (imports `Command`/`filterCommands`/
`isEnabled`, read-only) and `src/App.svelte`'s NavIntent's `startCommand` (via CPE-1552, type-only) but
edits neither. No overlap with CPE-1553 or CPE-1555's files. **Dispatch order:** after CPE-1552 (for the
`NavIntent`/`startCommand` type shape referenced in the ticket's design, though the component itself only
depends on `Command`); independent of, and mergeable in parallel with, CPE-1553 and CPE-1555.
