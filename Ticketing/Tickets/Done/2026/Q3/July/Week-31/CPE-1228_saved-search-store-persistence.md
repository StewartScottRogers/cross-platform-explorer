---
id: CPE-1228
title: "Saved-search store + persistence (wire up the orphaned savedSearch model)"
type: Task
priority: Medium
component: frontend
tags: [ready]
estimate: 1h
created: 2026-08-01
epic: CPE-978
closed:
---

## Context
`src/lib/savedSearch.ts` (CPE-986) has a pure, tested `SavedSearch` model + `evaluateSavedSearch` +
`serializeSavedSearch`/`parseSavedSearch`, but it's ORPHANED — no store, no persistence, no UI.
This ticket builds the FOUNDATION: a persisted store, mirroring the existing wired precedents
`src/lib/smartFolders.ts` and `watchRules.ts` (localStorage via `persist.ts`, tolerant parse).

## Acceptance criteria
- A `savedSearches` writable store (new file, e.g. `src/lib/savedSearchStore.ts`) initialized from
  `lsGet("cpe.savedSearches")` and persisting every change via `lsSet` (mirror `smartFolders.ts`
  ~lines 71-73), using the existing `serializeSavedSearch`/`parseSavedSearch` for round-trip.
- Helpers: add, rename, remove a `SavedSearch` (mirror `smartFolders.ts`'s `renameSaved`/`removeSaved`).
- Tolerant load (drop malformed entries, never throw).
- REAL vitest coverage: add/rename/remove, persistence round-trip, tolerant parse of junk.
- No UI yet (that's CPE-1229). Plain explorer unaffected when none defined.

## Notes
Foundation for CPE-1229 (UI wiring). Keep it a thin store over the existing pure model — do NOT
duplicate the evaluator or the Condition matcher.
