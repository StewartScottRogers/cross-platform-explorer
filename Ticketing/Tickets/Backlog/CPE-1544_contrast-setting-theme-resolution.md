---
id: CPE-1544
title: "High contrast: add ContrastSetting + theme.ts resolution, widen applyTheme to compose hc-* themes"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1496
created: 2026-08-09
---
## Context
CPE-1496's high-contrast slice needs a persisted preference orthogonal to the existing `ThemeSetting`
(`"system" | "light" | "dark"`, `src/lib/types.ts:23`): a user can want High Contrast independent of
which base theme they're on. This ticket adds that second axis and teaches `theme.ts` to compose it with
the existing light/dark resolution into the `hc-light`/`hc-dark` `data-theme` values CPE-1543 authors CSS
for. It is **inert on landing**: nothing calls the new APIs with a non-default value yet (that's
CPE-1545 for manual selection, CPE-1546 for the OS signal), so `applyTheme`'s existing call sites in
`src/main.ts` and `SettingsDialog.svelte` are unaffected — same "inert plumbing first" shape as
CPE-1540 in the dark-theme epic.

## Scope
- `src/lib/types.ts`: add `export type ContrastSetting = "system" | "off" | "high";` next to the existing
  `ThemeSetting` (line 23). `"off"` is the default (no contrast boost); `"high"` is an explicit manual
  override; `"system"` follows the OS high-contrast signal once CPE-1546 supplies one (until then it
  behaves identically to `"off"`, since no OS signal exists yet).
- `src/lib/settings.ts`: add `KEYS.contrast: "cpe.contrast"` alongside `KEYS.theme` (~line 84), an
  `isContrast` type guard next to `isTheme` (~line 185-186), and `loadContrast`/`saveContrast` next to
  `loadTheme`/`saveTheme` (~line 245-246), defaulting to `"off"` on a missing/corrupt value — same
  degrade-cleanly pattern as every other validated setting in this file.
- `src/lib/theme.ts`:
  - Add `resolveContrast(pref: ContrastSetting, osHighContrastActive = false): boolean` — `"high"` →
    `true`, `"off"` → `false`, `"system"` → returns `osHighContrastActive` (defaults `false`, so callers
    that don't pass it get today's behaviour).
  - Widen `applyTheme` to `applyTheme(themePref: ThemeSetting, contrastPref: ContrastSetting = "off",
    osHighContrastActive = false): void` — computes the base resolved theme via the existing
    `resolveTheme`, computes `hc = resolveContrast(contrastPref, osHighContrastActive)`, and stamps
    `document.documentElement.dataset.theme` to `` `hc-${base}` `` when `hc` is true, else `base`
    (unchanged). Both new parameters are optional with defaults that reproduce the current one-argument
    behaviour exactly, so every existing caller keeps compiling and passing.
  - Leave `resolveTheme` and `watchSystemTheme` untouched — this ticket only adds `resolveContrast` and
    widens `applyTheme`'s signature.

## How
- Extend `src/lib/theme.test.ts` with cases for `resolveContrast` (all three pref values, with and
  without `osHighContrastActive`) and `applyTheme`'s new hc-composition (e.g.
  `applyTheme("dark", "high")` stamps `"hc-dark"`; `applyTheme("light", "system", true)` stamps
  `"hc-light"`; `applyTheme("light", "system", false)` stamps `"light"`; a bare `applyTheme("dark")`
  still stamps `"dark"`, proving the default-args path is unchanged).
- Extend `src/lib/settings.test.ts` with the same load/save/corrupt-value-degrades-to-default coverage
  the existing `loadTheme`/`saveTheme` tests use, adapted for `loadContrast`/`saveContrast`.

## Verify
`npx vitest run src/lib/theme.test.ts src/lib/settings.test.ts`; `npm run check`. Fully headless — pure
TS logic, no DOM rendering beyond the existing `document.documentElement.dataset` assertions
`theme.test.ts` already exercises.

## Notes
**Conflict surface:** `src/lib/types.ts` (one new type, additive), `src/lib/settings.ts` (one new KEYS
entry + one new guard + two new exported functions, localized near the existing theme block at
~line 84 and ~line 241-246), `src/lib/theme.ts` (one new function + `applyTheme`'s signature widened
with backward-compatible defaults, localized to that one function), plus their two test files. No
`src/app.css`, `src/App.svelte`, `src/main.ts`, or `SettingsDialog.svelte` edits — this ticket is pure
plumbing. **Dispatch order:** independent — can run in parallel with CPE-1543 (no shared files).
CPE-1545 and CPE-1546 both depend on this landing first (they call `resolveContrast`/the widened
`applyTheme`).
