---
id: CPE-774
title: Pure rule-evaluation engine for file coloring & labels
type: feature
status: In Progress
priority: low
component: Frontend
tags: ready
created: 2026-07-20
closed:
epic: CPE-709
estimate: 1-2h
---

## Summary
Foundation for rules-based coloring & labels (epic CPE-709). A pure, dependency-light module
(`src/lib/colorRules.ts`) that, given a file entry and an ordered rule set, resolves the row's style —
no DOM, unit-tested — so the renderer (CPE-775) is a thin apply and the editor (CPE-776) a thin CRUD.

## Scope
- A `Condition` tagged union: `ext` (extension in list), `glob` (name matches a glob, reusing
  `matchesGlob`), `size` (min/max bytes), `olderThan`/`newerThan` (N days vs `entry.modified` epoch-ms),
  `isDir` (bool).
- `ColorRule = { id, when: Condition, color?, label?, enabled? }`.
- `matchesCondition(entry, cond, now): boolean` and `evaluateRules(entry, rules, now): { color?, label? }`
  — **first enabled matching rule wins** (returns `{}` if none match).
- Pure + total: null `modified`, empty rules, disabled rules, no-extension names all handled.

## Acceptance Criteria
- [x] Each condition kind matches correctly (incl. case-insensitive extension, glob, size bounds, age
      both directions, isDir); disabled rules are skipped.
- [x] `evaluateRules` returns the first matching enabled rule's style; `{}` when none match.
- [x] Pure + dependency-light; unit tests cover each kind + first-match precedence + edge cases;
      `npm run check` + suite green.

## Notes
Foundation for CPE-775 (renderer) / CPE-776 (editor). Reuses `src/lib/glob.ts`. Headless.

## Resolution
Added `src/lib/colorRules.ts` (pure): a `Condition` union (ext / glob / size / olderThan / newerThan /
isDir), `ColorRule = {id, when, color?, label?, enabled?}`, `matchesCondition(entry, cond, now)` and
`evaluateRules(entry, rules, now)` → the first enabled matching rule's `{color?, label?}` (or `{}`).
Reuses `matchesGlob`; extension match is case-insensitive with an optional leading dot and treats dotfiles
as extension-less; age conditions read `entry.modified` (epoch-ms, null never matches). 7 tests cover every
condition kind + first-match precedence + disabled-skip + empty-rules. check 0/0; suite 783. Headless; no
existing code touched. Foundation for CPE-775 (renderer) / CPE-776 (editor).

