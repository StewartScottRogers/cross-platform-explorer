---
id: CPE-610
title: Translate the search dialogs + command palette (i18n gap)
type: enhancement
component: Frontend
priority: low
status: Done
tags: ready
estimate: 3-4h
created: 2026-07-18
closed: 2026-07-18
---

## Summary
The app is broadly localised (12 COMPLETE_LOCALES; MenuBar, NavToolbar, Properties, Duplicates, and
BatchRename dialogs all use `$t`), but three surfaces were still **English-only**:
- `ContentSearchDialog.svelte` (0 `$t` uses) — pre-existing gap.
- `FileNameSearchDialog.svelte` (CPE-603) — new, same pattern.
- The command palette (CPE-602/605) labels + `CommandPalette.svelte` chrome ("Type a command…",
  "No matching commands.", group names).

## Acceptance Criteria
- [x] The two search dialogs and the palette use `$t(...)` for all user-visible strings.
- [x] New keys added to **every** COMPLETE_LOCALES block (the CPE-481 coverage gate holds each to 100%,
      so a partial add fails CI). AI-translated to match the CPE-539 quality bar; strings are simple UI
      phrases (Search, No matches, Find files by name), so low translation risk.
- [x] `npm run check` clean; the i18n coverage test passes; spot-check es in the running app.

## Notes
Deferred from unattended Nightshift: bulk-editing ~18 keys across 12 locale blocks in the 3k-line
`i18n.ts` is error-prone and the translations want a reviewable batch (as CPE-539 was), not an
unattended overnight edit. The palette portion is larger (~40 command labels) and could be split out if
this ticket runs long. Consistency-only — nothing is broken; untranslated locales already fall back to
English.

## Resolution (2026-07-18)
Split into two waves:
- **Search dialogs** were internationalised earlier under **CPE-619** (the `search.*` namespace across
  all 12 COMPLETE_LOCALES blocks; `ContentSearchDialog.svelte` + `FileNameSearchDialog.svelte` now use
  `$t`).
- **Command palette** completed here: added a `palette.*` namespace (**56 new keys**) to **all 12**
  COMPLETE_LOCALES blocks in `src/lib/i18n.ts`. `src/App.svelte`'s reactive `$: paletteCommands` array
  now resolves every `label:` and `group:` via `$t(...)` (labels update live on locale change), and
  `src/lib/components/CommandPalette.svelte` translates its chrome (input placeholder, empty-state, and
  both `aria-label`s).

Judgment calls:
- **Dynamic labels** (hide/show details, hide/show hidden, mix/group folders) each got **two keys** and
  keep their `showDetails ? … : …` ternaries, now selecting between two `$t(...)` calls.
- **Group headers** are translated (`palette.groupGo/File/View/Tools/App/Recent`); the recent-locations
  spread reuses `palette.groupRecent`.
- `keywords`, `id`, `run`, `enabled`, `shortcut`, ordering, and the `recentPaths(...)` spread were left
  untouched; only display strings changed. No reactive assignments were introduced inside the block, so
  no svelte-check cyclical-dependency error.
- Reused existing translations from sibling namespaces (`menu.*`, `ctx.*`, `mi.*`, `tb.*`, `search.*`)
  for consistency; crafted fresh natural translations for palette-only phrasing (e.g. "Up one folder",
  "Reopen closed tab", "Delete permanently", "Open terminal here", "View: …", "Sort by …").

Verification: `npm run check` → 0 errors / 0 warnings; `npx vitest run src/lib/i18n.test.ts` → 34/34
(coverage gate 100% for all COMPLETE_LOCALES); `npx vitest run` → 68 files / 648 tests all green.
