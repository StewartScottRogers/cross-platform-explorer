---
id: CPE-1627
title: "45 hardcoded-English showNotice() strings in App.svelte never translate, including three siblings of the notice CPE-1614 just fixed"
type: Task
status: Doing
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

## Work Log
- Confirmed the count: `grep -n 'showNotice("' src/App.svelte` finds exactly **45** occurrences (matching
  the ticket). All 45 converted — 0 left unconverted, 0 deliberately excluded. (There are also 62
  pre-existing `showNotice(\`templated ${x}\`)` calls with interpolated template literals — out of scope,
  not matched by the ticket's `showNotice("` grep, and left untouched; that's a separate debt.)
- 35 distinct translation keys cover the 45 call sites — several literals were byte-identical duplicates
  (e.g. "Couldn't open the Agent Board window." appeared 4×, "Couldn't copy the path to the clipboard."
  3×) and now share one key instead of getting a new one each, which also removes the duplication `grep`
  would otherwise have to wade through.
- The three `blockedInArchive()` siblings CPE-1614 flagged are now translated: `smart.searchBlockedNotice`
  (saved search), `archive.blockedNotice` (archive), `replay.blockedNotice` (Replay mode) — the latter is
  reused verbatim at its second call site in `importDroppedFiles` (identical English source text).
  `archive.blockedImportNotice` is a distinct fourth key for the drag-drop-specific archive wording ("...
  exit the archive to import files.") that isn't identical to the generic archive notice.
- New namespaces added to `src/lib/i18n.ts`: `notice.*` (16 keys, cross-cutting notices with no single
  feature home — clipboard/terminal/reveal/preview/save/export failures etc.), `archive.*` (2),
  `replay.*` (1). Reused existing namespaces where a clear feature affinity existed: `tb.*` (5, Agent
  Deck/Board — joins the existing `tb.openConsole`/`tb.aiConsole` etc.), `smart.*` (1, joins CPE-1614's
  `smart.blockedNotice`), `home.*` (3), `ctx.*` (3), `tags.*` (1), `link.*` (1), `xfer.*` (1),
  `search.*` (1). All 35 keys added to all 12 `COMPLETE_LOCALES` catalogs (en/es/de/fr/it/pt/nl/pl/ru/
  zh/ja/ko), each as a real, idiomatic translation (not English pasted into 12 slots) — reused existing
  catalog vocabulary for shared terms (clipboard/terminal/"reveal in file manager"/Settings/Repair/
  Recent/Tags/Undo/archive) so the new strings read consistently with what's already there. "Agent Deck"
  and "Agent Board" stay untranslated (proper nouns) in every locale, matching the existing
  `tb.aiConsole`/`palette.openAgentBoardWindow` convention already in the catalogs.
- Interpolated notices (`notice.macroLoadFailed`, `notice.autoSyncPaused`, `notice.autoSyncFailed`) use
  the catalogs' `{name}`-style placeholder convention (`translate()`'s `interpolate()`), not string
  concatenation — e.g. `$t("notice.autoSyncPaused", { reason })` instead of `"Auto-sync paused — " +
  reason`, so translated word order isn't broken.
- Fact-check per the ticket's ask: none of the 45 strings were factually wrong (the `blockedInArchive()`
  saved-search branch the ticket called out as already-correct was confirmed correct against its actual
  guard condition; the other conditions — batch-media count, shred-folders guard, preview-popout
  single-selection, etc. — all match their code). **No bug ticket filed** — this was a pure localisation
  pass with no copy rewrite needed.
- Added a regrowth guard: `src/App.showNoticeI18nGuard.test.ts` scans `App.svelte`'s source for
  `showNotice(` calls whose first argument starts with a raw `"` or `'` (i.e. bypasses `$t()`) and fails
  if any exist. Escape hatch: append `// i18n-exempt: <reason>` on the same line for a genuinely
  non-user-facing case (none needed today — all 45 are real, so the hatch is unused but available and
  documented in the test's own header comment). It can't be defeated by e.g. wrapping in `$t()` trivially
  since that's the actual fix, not a bypass; whitespace variants (`showNotice(  "x"`) still match.
- Added `src/App.blockedNoticesI18n.test.ts`: three tests (saved search / archive / Replay mode), each
  sets `locale` to German, opens the real feature (saved search via `addSavedSearch`+click, archive via
  a real zip double-click, Replay mode via the same watched-session → Replay-tab → "Show in file pane"
  path `App.replayGuards.test.ts` uses), triggers a blocked mutating action, and asserts the rendered
  notice equals `translate("de", <key>)` AND differs from the English string — proving these three
  actually render translated text, not just that the key resolves. Mirrors the CPE-1614 test's pattern
  (`App.smartFolderBlockedNotice.test.ts`).
- Verification (all run synchronously in the worktree):
  - `npm run check` — 0 errors, 0 warnings (ran twice, before and after the new test files).
  - `npx vitest run src/lib/i18n.test.ts` — 34/34 passed, including the "holds every locale declared
    complete to 100% coverage" guard.
  - `npx vitest run src/App.blockedNoticesI18n.test.ts` — 3/3 passed.
  - `npx vitest run src/App.showNoticeI18nGuard.test.ts` — 1/1 passed.
  - Full `npx vitest run` (twice — once right after the App.svelte text swap, once after adding the two
    new test files): 275 files / 3330 tests passed, 0 failures, 0 skipped.
  - No Rust touched — `cargo` steps not applicable.
