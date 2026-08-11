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
