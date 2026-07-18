---
id: CPE-610
title: Translate the search dialogs + command palette (i18n gap)
type: enhancement
component: Frontend
priority: low
status: Open
tags: ready
estimate: 3-4h
created: 2026-07-18
---

## Summary
The app is broadly localised (12 COMPLETE_LOCALES; MenuBar, NavToolbar, Properties, Duplicates, and
BatchRename dialogs all use `$t`), but three surfaces are still **English-only**:
- `ContentSearchDialog.svelte` (0 `$t` uses) — pre-existing gap.
- `FileNameSearchDialog.svelte` (CPE-603) — new, same pattern.
- The command palette (CPE-602/605) labels + `CommandPalette.svelte` chrome ("Type a command…",
  "No matching commands.", group names).

## Acceptance Criteria
- [ ] The two search dialogs and the palette use `$t(...)` for all user-visible strings.
- [ ] New keys added to **every** COMPLETE_LOCALES block (the CPE-481 coverage gate holds each to 100%,
      so a partial add fails CI). AI-translated to match the CPE-539 quality bar; strings are simple UI
      phrases (Search, No matches, Find files by name), so low translation risk.
- [ ] `npm run check` clean; the i18n coverage test passes; spot-check es in the running app.

## Notes
Deferred from unattended Nightshift: bulk-editing ~18 keys across 12 locale blocks in the 3k-line
`i18n.ts` is error-prone and the translations want a reviewable batch (as CPE-539 was), not an
unattended overnight edit. The palette portion is larger (~40 command labels) and could be split out if
this ticket runs long. Consistency-only — nothing is broken; untranslated locales already fall back to
English.
