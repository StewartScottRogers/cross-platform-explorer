---
id: CPE-049
title: Sort the file list in natural (numeric-aware) order like Explorer
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The file list is sorted with a plain `String.localeCompare`, so names containing numbers sort
lexicographically: `file10.txt` comes before `file2.txt`, and `IMG_9.jpg` before `IMG_10.jpg`.
Windows Explorer (and macOS Finder) use **natural / numeric-aware** ordering, where embedded numbers
compare by value. This ticket makes the file list match that expectation.

The sort/compare logic currently lives inline in `App.svelte` (a reactive `$:` block) with no unit
tests. As part of this feature, extract it into a dedicated, tested `src/lib/sort.ts` module.

## Acceptance Criteria

- [ ] Names are sorted numerically-aware: `file2` before `file10`; `img9` before `img10`
- [ ] Directories still sort before files (existing rule preserved)
- [ ] All four sort keys (name / modified / type / size) and both directions still work
- [ ] `type` and `size` sorts keep a natural-name tiebreaker
- [ ] Sort logic extracted to `src/lib/sort.ts` and consumed by `App.svelte`
- [ ] Unit tests in `src/lib/sort.test.ts`; `npm run check` clean; full suite green

## Resolution

Extracted the file-list sort into `src/lib/sort.ts` (`compareNames`, `compareEntries`, `sortEntries`)
using `Intl.Collator(undefined, { numeric: true, sensitivity: "base" })` for numeric-aware ordering.
`App.svelte` now calls `sortEntries(filtered, sortKey, sortDir)` (removed the now-unused `typeName`
import). Directories-first preserved; type/size keys keep a natural-name tiebreaker. Added 10 unit
tests. `npm run check` = 0 errors; full suite 101 passed; `vite build` clean. Committed on branch
`cpe-049-natural-sort`, merged to `main`, pushed.

## Work Log

2026-07-11 — Nightshift loop: researched feature gaps; picked natural-sort as highest value (visible, matches Explorer, strong unit-testable core). Confirmed current inline sort in App.svelte uses plain localeCompare. Types SortKey/SortDir/DirEntry verified.
2026-07-11 — Implemented sort.ts + tests; wired into App.svelte. check 0 errors, 101 tests pass, vite build clean. GUI verify (drive the app, confirm file2 before file10 in the list) DEFERRED — user present, GUI paused per Nightshift rules.

## Notes

Natural ordering via `Intl.Collator(undefined, { numeric: true, sensitivity: "base" })`, reused for the
`type`-sort name tiebreaker too.
