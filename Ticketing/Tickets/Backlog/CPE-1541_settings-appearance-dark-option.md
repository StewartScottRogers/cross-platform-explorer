---
id: CPE-1541
title: "Dark theme: Settings -> Appearance gains the \"Dark\" theme option"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1493
created: 2026-08-09
---
## Context
`SettingsDialog.svelte`'s Appearance section (added in CPE-1536) already has the exact extension point
its own comment describes: *"the options list is kept as an array so adding 'Dark' later is a one-line
change"* (`src/lib/components/SettingsDialog.svelte:38-45`, the `THEME_OPTIONS` array). CPE-1540 widens
`ThemeSetting` to include `"dark"`; this ticket is the one-line-array change that comment was written for,
plus refreshing the note text underneath the select. **Depends on CPE-1540** — `THEME_OPTIONS` is typed
`{ value: ThemeSetting; label: string }[]`, so a `"dark"` entry only typechecks once `ThemeSetting`
includes it.

## Scope
- `src/lib/components/SettingsDialog.svelte:42-45` (`THEME_OPTIONS`): add `{ value: "dark", label: "Dark"
  }` as a third option (System / Light / Dark). No change to `setTheme`/`onThemeChange` — they're already
  generic over `ThemeSetting`.
- `src/lib/components/SettingsDialog.svelte:125-127` (the `.note` line under the select): replace "Light
  is the only theme today — System will follow your OS automatically once dark mode ships." with copy
  reflecting that System now follows the OS live and Dark can be picked explicitly (per
  [[prefer-inline-instant-controls]] — no separate save step, matches the row above it).
- Keep the inline-instant-control pattern already in place — no modal, no separate Apply button.

## How
- Add the array entry + note-text edit only; the select, `applyTheme`/`saveTheme` wiring, and dataset
  attribute stamping are all already generic and need no change.
- Extend `src/lib/components/SettingsDialog.test.ts` (see its existing `"changing the select persists via
  saveTheme and applies via applyTheme"` test) with a case selecting `"dark"` and asserting
  `saveTheme("dark")` + `applyTheme("dark")` are both called, and that the `<select>` now renders three
  `<option>`s.

## Verify
`npx vitest run src/lib/components/SettingsDialog.test.ts`; `npm run check`. Fully headless — the
component test drives the `<select>` via its `data-testid="theme-select"` handle already used by the
existing test, no real browser/GUI needed.

## Notes
**Conflict surface:** `src/lib/components/SettingsDialog.svelte` (only the `THEME_OPTIONS` array,
`src/lib/components/SettingsDialog.svelte:42-45`, and the `.note` copy at `:125-127` — no other part of
that file) plus its test file. **Dispatch order:** after CPE-1540 (needs the widened `ThemeSetting`).
Independent of CPE-1539 (dark CSS) at the compile level — this ticket only changes which `<option>`s
exist and what string is persisted, not any styling — though the option is visually meaningless until
CPE-1539 + CPE-1540 are both live.
