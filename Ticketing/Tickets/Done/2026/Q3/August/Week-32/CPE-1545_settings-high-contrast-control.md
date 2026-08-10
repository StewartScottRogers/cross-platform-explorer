---
id: CPE-1545
title: "Settings -> Appearance: add a High Contrast control (manual override) + refresh the doc page"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1496
created: 2026-08-09
---
## Context
CPE-1544 lands `ContrastSetting`, `resolveContrast`, and a widened `applyTheme` as inert plumbing.
This ticket is the visible entry point: a second control in Settings → Appearance (`SettingsDialog.svelte`,
which already has a `Theme` select at `src/lib/components/SettingsDialog.svelte:117-128`, added by
CPE-1536/CPE-1541) that lets the user turn High Contrast Off / System / High, independent of their
Light/Dark/System theme choice. Depends on CPE-1544 for the types/functions it calls; independent of
CPE-1543's CSS (an unmatched `data-theme="hc-*"` value degrades harmlessly to the bare `:root` block
until CPE-1543 lands, so this ticket's own build/tests don't need to wait on it).

## Scope
- `src/lib/components/SettingsDialog.svelte`: inside the existing `Appearance` section
  (`section-title` at line 117, right after the `Theme` row's `<div class="note">` at line 126-128), add
  a second `settings-row` with a `Contrast` label and a `<select data-testid="contrast-select">` offering
  Off / System / High, mirroring the existing `THEME_OPTIONS`/`onThemeChange` pattern exactly:
  a `CONTRAST_OPTIONS: { value: ContrastSetting; label: string }[]` array, a `contrast` local reactive
  var seeded from `settings.loadContrast()`, a `setContrast(v)` that saves + calls the widened
  `applyTheme(theme, v)` (passing the current `theme` value so the composed `hc-*` stamp is correct), and
  an `onContrastChange` change handler — same inline-instant-control shape as the theme select
  ([[prefer-inline-instant-controls]]). A one-line note under it (mirrors the existing theme note) explaining
  System follows the OS accessibility high-contrast signal once available, High always forces it, Off never
  does.
- Keep this additive — do not restructure the existing `Theme` row or its handlers beyond passing the new
  `contrast` value through to `applyTheme` at its two existing call sites (`setTheme`'s `applyTheme(v)` call
  becomes `applyTheme(v, contrast)`).
- `src/docs/35-appearance.md`: extend the existing **Appearance** page (already anticipates this — its
  "What's next" section literally says "a high-contrast option are still to come"). Add a `## The Contrast
  control` section documenting Off/System/High the same way the existing Theme section is documented, and
  trim the "What's next" line's high-contrast mention now that it has shipped. No `sectionDocs.ts` change —
  the `appearance` section is already registered (CPE-1542).

## How
- Extend `SettingsDialog.test.ts` with the same shape of coverage the existing theme-select tests use
  (`src/lib/components/SettingsDialog.test.ts:43-86`): the contrast select reflects
  `settings.loadContrast()` on mount; changing it persists via `saveContrast` and applies via the widened
  `applyTheme` (assert `document.documentElement.dataset.theme` gets the `hc-` prefix when High is chosen
  while the theme is e.g. `"dark"`, and loses it when switched back to Off); it offers exactly Off/System/High.

## Verify
`npx vitest run src/lib/components/SettingsDialog.test.ts`; `npm run check`. Fully headless —
jsdom component test, no real GUI needed. Keyboard-reachability of the new `<select>` is free (native
`<select>` elements are keyboard-operable by default; no custom widget introduced).

**Async visual sign-off queued (not headless):** the same aesthetic pass queued in CPE-1543 applies once
this control is wired to a landed hc-* palette — not blocking for this ticket, which only needs the
control to compose the correct `data-theme` string.

## Notes
**Conflict surface:** `src/lib/components/SettingsDialog.svelte` (small, localized addition inside the
existing `Appearance` section, `src/lib/components/SettingsDialog.svelte:117-128` region — no other
section of the file touched), its test file, and `src/docs/35-appearance.md` (additive section + one
line trimmed). No `src/app.css`, `src/App.svelte`, or `src/lib/theme.ts`/`types.ts`/`settings.ts` edits
beyond what CPE-1544 already landed. **Dispatch order:** prereq CPE-1544 (needs `ContrastSetting`,
`loadContrast`/`saveContrast`, `resolveContrast`, widened `applyTheme`). Can run in parallel with
CPE-1543 and CPE-1546 once CPE-1544 is in.
