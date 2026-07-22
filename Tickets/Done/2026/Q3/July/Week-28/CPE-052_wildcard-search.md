---
id: CPE-052
title: Support wildcard (glob) patterns in the file search box
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The in-folder search filters entries with a plain case-insensitive substring match
(`name.toLowerCase().includes(query)`). Windows Explorer lets you type wildcards: `*.txt` for every
text file, `report?.md` for `report1.md`/`reportA.md`, etc. Add wildcard support: when the query
contains `*` or `?`, treat it as a glob matched against the whole name; otherwise keep the existing
substring behaviour. Extract the match logic into a tested `src/lib/search.ts`.

## Acceptance Criteria

- [ ] A query with no wildcards still matches as a case-insensitive substring (unchanged)
- [ ] `*` matches any run of characters; `?` matches exactly one
- [ ] Wildcard queries match against the full name (anchored), e.g. `*.txt` matches `a.txt` not `a.txtx`
- [ ] Regex metacharacters in the query (`.`, `(`, `+`, …) are treated literally
- [ ] Matching is case-insensitive
- [ ] Logic lives in `src/lib/search.ts` and is consumed by `App.svelte`
- [ ] Unit tests added; `npm run check` clean; full suite green

## Resolution

Added `matchesQuery(name, query)` in `src/lib/search.ts`: plain queries keep the case-insensitive
substring match; queries containing `*`/`?` are compiled to an anchored RegExp (metacharacters escaped,
`*`→`.*`, `?`→`.`) and matched against the whole name. `App.svelte`'s filter now calls it. 6 unit
tests. `npm run check` 0 errors; suite 126 passed; `vite build` clean. Committed on branch, merged to
`main`, pushed. GUI verify (type `*.md` in the search box) DEFERRED — user present.

## Work Log

2026-07-11 — Nightshift loop: research picked wildcard search as the next feature (visible, Explorer-parity, strong pure core). Plan: glob→regex with metacharacter escaping, anchored full-match, substring fallback when no wildcard present.

## Notes

Full-match-for-wildcards vs. substring-for-plain-text mirrors Explorer's behaviour.
