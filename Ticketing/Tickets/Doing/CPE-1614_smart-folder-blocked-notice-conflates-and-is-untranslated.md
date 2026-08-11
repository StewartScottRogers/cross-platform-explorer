---
id: CPE-1614
title: "The \"can't edit here\" smart-folder notice calls it a saved search — and is hardcoded English"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Found by the independent UAT tester on CPE-1605/CPE-1612 (PR #813) — outside that PR's scope, pre-existing
on `main`, and genuinely two bugs in one line.

`src/App.svelte:1882` — the notice shown when you try to rename, move, delete, copy or paste inside a smart
folder reads:

> This is a smart folder — a saved search view.

Two problems:
1. **It conflates the two features**, in exactly the way CPE-1605 was filed to stop — just in a different
   surface. A smart folder is a single-tag live view; a saved search is a multi-condition query scoped to a
   captured root. The app's own documentation (`explorer-smart-folders.md` / `explorer-saved-searches.md`,
   shipped today) draws that distinction clearly, and each page names the other precisely so a user can
   choose between them. This toast then tells them they're the same thing.
2. **It is hardcoded English** — not routed through `$t()` at all, so it stays English in all twelve
   locales while the surrounding UI translates.

## Fix
- Reword so it names only what the surface actually is, and stays consistent with the sidebar tooltip
  CPE-1605 just corrected and with the docs pages.
- Route it through `$t()` with a new key, translated across **all 12 locale catalogs** (the coverage guard
  test enforces this).
- Check for siblings while you're there: grep `App.svelte` and the components for other hardcoded
  user-facing strings that bypass `$t()`, and for any other place the two features are used
  interchangeably. Report what you find even if you don't fix it all — a list is useful.

## Acceptance criteria
- The blocked-action notice names the surface correctly and matches the terminology used in the sidebar and
  the docs.
- The string is translated in all 12 locales; the i18n guard test passes.
- A test asserts the notice renders (and is translated) rather than only that it exists in the catalogue.

## Notes
Small. Conflict surface: `src/App.svelte`, `src/lib/i18n.ts`, and the relevant test.
Related: [[CPE-1605]]. Model: sonnet (or haiku — mechanical, but the wording needs a moment's care).

## Work Log
- Reworded `blockedInArchive()`'s smart-folder notice in `src/App.svelte` (was: "This is a smart folder —
  a saved search view. Open a file's real location to change it.") to name only what a smart folder
  actually is, matching the CPE-1605-corrected sidebar tooltip and `explorer-smart-folders.md`'s "live
  view of tagged files" language: "This is a smart folder — a live view of tagged files. Open a file's
  real location to change it." Routed through a new key `smart.blockedNotice`, added to all 12 full
  catalogs in `src/lib/i18n.ts` (en/es/de/fr/it/pt/nl/pl/ru/zh/ja/ko — the only 12 that exist as full
  `messages` blocks, matching `COMPLETE_LOCALES`).
- The saved-search sibling notice one branch below (`structuredSearch`) already said "a saved search — a
  read-only view" and does not conflate the two features — left as-is (correct wording), though it and the
  archive/Replay-mode notices in the same function are still hardcoded English (not part of this ticket's
  scope; noted below as a sibling finding).
- Updated `src/docs/explorer-smart-folders.md` (CPE-579): the "What you can't do inside one" section quoted
  the old buggy notice text verbatim — updated the quoted string to match the corrected copy.
- Added `src/App.smartFolderBlockedNotice.test.ts`: opens a real smart folder via `saveSmartFolder`, fires
  a `Delete` keydown (blockedInArchive() fires before the selection-empty check in `askDelete`, so no
  selection is needed), and asserts the rendered notice equals `translate("en", "smart.blockedNotice")`,
  does not contain "saved search", and does contain "smart folder" — proving it renders through `$t()`
  rather than just existing in the catalog.
- Sibling hardcoded-string audit (per the ticket's ask — reporting, not fixing, since out of scope): grep
  of `showNotice("` literals in `src/App.svelte` finds **45** occurrences total, including the three other
  branches of `blockedInArchive()` itself (archive read-only notice, saved-search notice, Replay-mode
  notice) — all hardcoded English, none conflate smart-folder/saved-search. No other conflation of the
  two features (smart folder vs. saved search) was found elsewhere in `App.svelte` or the docs.
- Verification (all run synchronously in the worktree):
  - `npm run check` — 0 errors, 0 warnings.
  - `npx vitest run src/App.smartFolderBlockedNotice.test.ts src/lib/i18n.test.ts src/lib/components/Sidebar.test.ts src/App.smartFolderLiveRefresh.test.ts src/App.savedSearch.test.ts` — all 5 files, 75 tests passed (i18n coverage guard included).
  - Full `npx vitest run` — 273 files / 3312 tests passed, 0 failures.
  - No Rust touched — `cargo` steps not applicable to this ticket.
