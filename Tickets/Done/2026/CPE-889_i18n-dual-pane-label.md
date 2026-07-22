---
id: CPE-889
title: i18n-ify the dual-pane toggle palette label (follow-up to CPE-677)
type: chore
component: Frontend
priority: low
tags: ready
epic: CPE-617
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
CPE-677/678 shipped the dual-pane/commander palette labels as hardcoded English strings (logged follow-up:
adding keys ×12 complete locales was disproportionate mid-loop). This closes the primary one: the toggle
label now goes through `$t` with `palette.dualPane` / `palette.singlePane` translated across all 12 complete
locales (en/es/de/fr/it/pt/nl/pl/ru/zh/ja/ko), so it's no longer English-only in a localized build.

The four commander **sub-command** labels (Copy/Move/Swap/Mirror to other pane) remain hardcoded for now —
they only appear in the palette while dual-pane is active; a lower-priority follow-up can localize those too.

## Acceptance Criteria
- [x] `view.dualPane` palette label uses `$t("palette.dualPane"/"palette.singlePane")`.
- [x] Both keys present + translated in all 12 complete locales; i18n coverage stays 1.0.
- [x] `npm run check` 0/0; i18n suite 34/34.

## Work Log
- 2026-07-22 (nightshift) — Added `palette.dualPane`/`palette.singlePane` to the 12 complete-locale catalogs
  with translations, wired the palette command to `$t`. i18n parity gate green (coverage 1.0 for every
  complete locale). Sub-command labels tracked as a minor future follow-up.
