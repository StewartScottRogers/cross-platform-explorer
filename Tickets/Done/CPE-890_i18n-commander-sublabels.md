---
id: CPE-890
title: i18n-ify the commander sub-command palette labels (F5/F6/swap/mirror)
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
Finishes the i18n follow-up from CPE-677/678/889. The four commander palette labels (Copy to other pane /
Move to other pane / Swap panes / Mirror path to other pane) now go through `$t` with
`palette.paneCopy`/`paneMove`/`paneSwap`/`paneMirror`, translated across all 12 complete locales. With
CPE-889 (the toggle label), the entire dual-pane/commander palette surface is now localized — no hardcoded
English strings remain in it.

## Acceptance Criteria
- [x] The four `view.pane*` palette labels use `$t(...)`.
- [x] All four keys present + translated in the 12 complete locales; i18n coverage stays 1.0.
- [x] `npm run check` 0/0; i18n suite 34/34.

## Work Log
- 2026-07-22 (nightshift) — Added `palette.paneCopy/paneMove/paneSwap/paneMirror` to the 12 complete-locale
  catalogs (F5/F6/Ctrl+U hints kept inline), wired the four commander palette commands to `$t`. i18n parity
  green. Closes the last of tonight's commander i18n debt.
