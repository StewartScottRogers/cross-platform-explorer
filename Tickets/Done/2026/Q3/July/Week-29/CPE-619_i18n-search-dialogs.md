---
id: CPE-619
title: i18n the search dialogs
type: enhancement
component: Frontend
priority: low
status: Done
tags: ready
estimate: 1-2h
created: 2026-07-18
closed: 2026-07-18
---

# CPE-619 — i18n the search dialogs

## Summary

Internationalize the two previously English-only search dialogs so every
user-visible string flows through the i18n `$t(...)` helper:

- `src/lib/components/ContentSearchDialog.svelte` (search inside files)
- `src/lib/components/FileNameSearchDialog.svelte` (find files by name)

A new `search.*` key namespace was added to **every** locale catalog in
`src/lib/i18n.ts` so the CPE-481/CPE-539 coverage gate stays at 100% for all
declared-complete locales.

This is a scoped slice of **CPE-610** (i18n the search dialogs *and* the
command palette). CPE-610 still tracks the remaining **command-palette**
portion — that component was intentionally left untouched here.

## Acceptance Criteria

- [x] `ContentSearchDialog.svelte` uses `$t(...)` for all visible text (title,
      placeholders, buttons, summary, empty/loading states, tooltips).
- [x] `FileNameSearchDialog.svelte` uses `$t(...)` for all visible text.
- [x] New `search.*` keys added to all 12 locale blocks in `i18n.ts`
      (en, es, de, fr, it, pt, nl, pl, ru, zh, ja, ko).
- [x] `npm run check` — 0 errors / 0 warnings.
- [x] `npx vitest run src/lib/i18n.test.ts` — coverage gate passes.
- [x] `npx vitest run` — full frontend suite green.
- [x] No visual/behavioral change beyond translation; same invoke commands.

## Resolution

Added `import { t } from "../i18n"` to both dialogs and replaced every hardcoded
English literal with `$t("search.*")` (reusing `common.close`, `home.expand`,
`home.collapse` where an exact key already existed). Pluralized counts are
composed from `search.matchOne/matchMany`, `search.fileOne/fileMany`, and a
`search.matchesInFiles` template so per-language word order is preserved and
interpolation keeps working.

Added 20 new `search.*` keys to each of the 12 complete locale catalogs, with
natural translations per locale (AI-translated to the CPE-539 quality bar). Also
extended the CPE-481 migration guard map in `i18n.test.ts` to cover both new
dialogs so a hardcoded literal can't regress back in.

## Work Log

- 2026-07-18: Studied i18n layout (DuplicatesDialog/PropertiesDialog patterns,
  COMPLETE_LOCALES, coverage gate). Wired both dialogs to `$t`. Added the
  `search.*` namespace to all 12 locale blocks. Added guard entries. All three
  verify commands green. Closed.
</content>
</invoke>
