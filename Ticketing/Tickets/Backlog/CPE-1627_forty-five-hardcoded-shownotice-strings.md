---
id: CPE-1627
title: "45 hardcoded-English showNotice() strings in App.svelte never translate, including three siblings of the notice CPE-1614 just fixed"
type: Task
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the worker on CPE-1614 (PR #816) and confirmed by that PR's reviewer. CPE-1614 routed ONE notice
through `$t()` and translated it across all 12 catalogs; a grep for `showNotice("` in `src/App.svelte`
then turned up **45** hardcoded-English literals still bypassing i18n entirely.

Among them are the other three branches of the very same `blockedInArchive()` function — the archive
read-only, saved-search, and Replay-mode notices. None of them *conflate* features (the saved-search one
correctly reads "This is a saved search — a read-only view"), so this is i18n debt rather than a
correctness bug. But it means a user running the app in any of the other 11 languages hits English text at
exactly the moments the app is trying to explain why something didn't work.

## Goal
Route every user-facing `showNotice(...)` literal in `App.svelte` through `$t()` with a translated key, so
the notice layer is fully localised and the coverage guard keeps it that way.

## Scope
- Enumerate all 45 (the grep is `showNotice("` in `src/App.svelte`) and give each a key.
- Translate across all 12 complete catalogs in `src/lib/i18n.ts` (`COMPLETE_LOCALES`); the coverage guard
  test in `i18n.test.ts` ("holds every locale declared complete to 100% coverage") enforces this.
- Where a notice interpolates a value, use the catalogs' existing placeholder convention — never
  concatenate translated fragments, which breaks word order in several of these languages.
- Keep wording as-is unless a string is factually wrong; this is a localisation pass, not a copy rewrite.
  Anything that turns out to be wrong earns its own ticket, exactly as CPE-1614 did.
- Consider a guard test that fails on a newly-added hardcoded literal, so the debt cannot silently regrow.

## Acceptance criteria
- No user-facing `showNotice` literal remains in `App.svelte`; all 12 catalogs at 100%, guard green.
- Switching locale changes every one of those notices; a test covers at least the `blockedInArchive()` set
  in a non-English locale.
- No behavioural change beyond the text being translated.

**Conflict surface:** `src/App.svelte`, `src/lib/i18n.ts` (12 catalogs), notice-related tests. Large but
mechanical, and it touches `App.svelte` heavily — do NOT run it in parallel with other `App.svelte` work.
