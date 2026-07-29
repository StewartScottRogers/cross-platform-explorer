---
id: CPE-986
title: Saved-search core — pure headless model + evaluator
type: feature
component: Frontend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-978
---

# CPE-986 — Saved-search core (pure headless model + evaluator)

## Summary

A pure, headless saved-query model + evaluator in `src/lib/savedSearch.ts`, the
foundation for epic CPE-978 "Smart folders & saved searches". A `SavedSearch` is a
serialisable, named bundle of `Condition`s combined with all/any; `evaluateSavedSearch`
filters a directory listing through it. No DOM/IO — the store and editor UI (later
tickets) are thin wrappers.

## Design

- **Reuse, don't reinvent.** The `Condition` type and its matcher `matchesCondition`
  already live in `src/lib/colorRules.ts` (CPE-774) and are reused by `selectMatch.ts`
  and `watchRules.ts`. `savedSearch.ts` imports `matchesCondition`, `isValidCondition`,
  and `type Condition` from there — no parallel matcher.
- **Model:** `SavedSearch = { id, name, conditions: Condition[], match: 'all' | 'any' }`.
  Kept minimal — no `scope`/`sort` added (existing sibling models don't carry them; out
  of scope for a pure core ticket).
- **Evaluation semantics:** `match: 'all'` = AND (empty conditions → matches everything,
  vacuous truth, i.e. an unfiltered smart folder); `match: 'any'` = OR (empty conditions →
  matches nothing). `evaluateSavedSearch` returns matching `DirEntry`s (order preserved),
  mirroring the "filtered listing" a smart folder consumes.
- **Persistence:** `serializeSavedSearch` (JSON) + `parseSavedSearch` — a tolerant guarded
  parse that returns `null` (never throws) on bad JSON / wrong shape / missing or blank
  name / a corrupted condition. Reuses `isValidCondition` so a landmine like `{kind:"ext"}`
  (no `exts`) is dropped at parse rather than throwing later, matching the `watchRules.ts`
  / `colorRulesStore` convention.

## Acceptance Criteria

- [x] `SavedSearch` type defined: `{ id, name, conditions: Condition[], match: 'all'|'any' }`.
- [x] Reuses the existing `Condition` type + `matchesCondition` from `colorRules.ts`
      (no reinvented file matching).
- [x] `evaluateSavedSearch(entries, search, now)` combines conditions via all/any.
- [x] `serializeSavedSearch` / `parseSavedSearch` round-trip; parse returns `null` on
      malformed JSON and on missing/blank name; never throws.
- [x] Tests in `src/lib/savedSearch.test.ts` cover all/any, some/none/empty, round-trip,
      malformed + blank-name rejection, and composition with a real (ext + date) condition.
- [x] `npm run check` passes (0 errors).
- [x] `npx vitest run src/lib/savedSearch.test.ts` passes.

## Work Log

- 2026-07-24 — Read `selectMatch.ts`, `colorRules.ts`, `watchRules.ts`. Confirmed the
  canonical `Condition` type + `matchesCondition` + `isValidCondition` live in
  `src/lib/colorRules.ts`; reused them directly.
- 2026-07-24 — Wrote `savedSearch.ts` (model, `matchesSavedSearch`, `evaluateSavedSearch`,
  serialise/guarded-parse) and `savedSearch.test.ts` (9 tests).
- 2026-07-24 — Verified: `npx vitest run src/lib/savedSearch.test.ts` → 9/9 pass;
  `npm run check` → 0 errors, 0 warnings.

### Assumptions logged

- Kept the model minimal — did **not** add `scope`/`sort`. Sibling models (`ColorRule`,
  `WatchRule`) don't carry them and the ticket says add only if trivial + consistent;
  they're better placed on the smart-folder store/UI tickets. Noted for the epic.
- `evaluateSavedSearch` returns matching `DirEntry[]` (not indices). `selectMatch.ts`
  returns indices for a *selection* move, but a saved search yields a filtered *listing*,
  so returning entries is the right shape for a smart folder. Also exported
  `matchesSavedSearch` (single-entry predicate) for reuse.
- `'all'` + empty conditions = match everything (vacuous AND); `'any'` + empty = match
  nothing. Documented in the type doc-comment.
