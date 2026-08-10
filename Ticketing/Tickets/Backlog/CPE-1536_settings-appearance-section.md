---
id: CPE-1536
title: "Theme foundation: Settings → Appearance section (system | light stub)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1492
created: 2026-08-09
---
## Context
CPE-1492's third slice: a visible entry point for the theme choice, per [[prefer-inline-instant-controls]]
and [[avoid-modal-permission-popups]] (the control belongs in Settings, not a launch-time prompt). Today
it's a stub — `system` and `light` behave identically (CPE-1535 resolves both to `"light"`) — but the
control exists now so CPE-1493 (OS light/dark) only has to add a real dark branch behind it, not build
the UI.

## Scope
- Add an **"Appearance"** `section-title` block to `src/lib/components/SettingsDialog.svelte`, following
  the existing self-contained-toggle pattern used by "Native metadata bridge" (`:94-107`) and "System
  tray" (`:132-146`) — reads/writes `settings.ts` directly, not via a prop from `App.svelte`.
- An inline `<select>` (not a modal) with two options, `System` / `Light`, bound to
  `settings.loadTheme()` / `settings.saveTheme()` (from CPE-1535) — changing it also calls
  `applyTheme(v)` (from CPE-1535) so the (currently inert) `data-theme` attribute updates live, same
  instant-apply feel as the other toggles on this page.
- A one-line `<div class="note">` explaining the stub honestly: e.g. "Light is the only theme today —
  System will follow your OS automatically once dark mode ships." (Do not imply dark mode already
  works.)
- Place it near the top of the dialog (general/appearance-adjacent settings), not nested under an
  unrelated section.

## How
- Import `loadTheme`/`saveTheme`/`applyTheme` (`settings.ts` re-exports or straight from `theme.ts`,
  whichever CPE-1535 lands as the public surface) at the top of the `<script>` block alongside the other
  `settings.*` imports.
- Mirror the exact local-state shape of `nativeBridgeEnabled`/`setNativeBridgeEnabled`
  (`SettingsDialog.svelte:39-43`): `let theme = settings.loadTheme(); function setTheme(v) { theme = v;
  settings.saveTheme(v); applyTheme(v); }`.
- No new dependency. No `App.svelte` changes — this dialog is self-contained for this control, same as
  the native-bridge/vault toggles.

## Verify
`npm run check`. Extend `src/lib/components/SettingsDialog.test.ts` (or create it if it doesn't exist yet
— check first) with cases: the select shows the persisted value on mount; changing it calls
`saveTheme`/`applyTheme` with the new value. Fully headless — jsdom + `@testing-library/svelte` (or
whatever harness the existing SettingsDialog tests use, if any — otherwise a thin new test file following
another dialog's test pattern, e.g. `ScheduledSnapshots.test.ts` if present). No GUI verification required
to land it (queue an async visual check since it's a new row in an existing dialog).

## Notes
**Conflict surface:** `src/lib/components/SettingsDialog.svelte` (additive — one new `section-title`
block, same pattern as five existing ones in the file, inserted without touching neighboring sections)
and its test file. No `src/App.svelte` edits. **Dispatch order: after CPE-1535** (needs
`loadTheme`/`saveTheme`/`applyTheme` to exist). Independent of CPE-1534 (different file).
