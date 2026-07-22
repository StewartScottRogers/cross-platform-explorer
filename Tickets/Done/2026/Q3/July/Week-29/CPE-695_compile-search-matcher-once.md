---
id: CPE-695
title: Compile the search matcher once per filter pass, not once per entry
type: enhancement
component: Frontend
priority: medium
status: Done
created: 2026-07-18
closed: 2026-07-18
epic: CPE-688
estimate: 1h
---

## Summary
Child of CPE-688 (10× file-list perf). The in-folder filter (`App.svelte:1101`) calls
`matchesQuery(e.name, search)` for **every** entry on **every keystroke**. For a glob query (`*`/`?`) that
recompiles a brand-new `RegExp` for each of the N entries — N regex compilations per keystroke — and it
re-normalizes the query string (`trim().toLowerCase()`) N times too. Compile the matcher **once** per
filter pass into a reusable predicate, so the per-entry cost is just the match.

Headless and behaviour-preserving: pure logic in `src/lib/search.ts`, verified by `npm run check` + the
existing/extended `search.test.ts`. No GUI needed (this is the filter *computation*), so safe to land
unattended.

## Acceptance Criteria
- [x] `search.ts` exposes a `makeMatcher(query)` that normalizes/compiles once and returns `(name) => boolean`.
- [x] `matchesQuery` is preserved (single-shot callers) with identical results, implemented via the matcher.
- [x] App's filter compiles the matcher once per search change (a reactive), not once per entry.
- [x] A test proves the glob regex is compiled once regardless of entry count.
- [x] `npm run check` + full suite green.

## Work Log
2026-07-18 (nightshift) — Picked up. Estimate: 1h. Waterfall at "create new work" (Doing/Backlog gated, no
Proposed epics). Diagnosed `App.svelte:1101`: `matchesQuery(e.name, search)` per entry recompiles the
glob→RegExp for every entry on every keystroke and re-normalizes the query N times.

2026-07-18 — `search.ts`: extracted `makeMatcher(query): (name) => boolean` that trims/lowercases and
compiles the glob RegExp **once**, returning a closure that only tests. Re-expressed `matchesQuery` as
`makeMatcher(query)(name)` — identical behaviour, single source of truth for the matching rules.
`App.svelte`: `$: searchMatcher = makeMatcher(search)` (compiled once per query change), and `filtered`
filters with `searchMatcher(e.name)`; swapped the import.

2026-07-18 — Tests (`search.test.ts`): parity test (makeMatcher ≡ matchesQuery over plain/glob/`?`/empty
cases) + two RegExp-constructor-spy tests proving a glob compiles exactly once across many names, and a
plain query constructs no RegExp at all. `npm run check` clean (0/0). Full suite green: 699 tests / 74
files (+3).

## Resolution
The in-folder filter recompiled a fresh glob `RegExp` for every entry on every keystroke (N compilations
per keystroke) and re-normalized the query N times. Added `makeMatcher(query)` to `src/lib/search.ts`,
which normalizes and compiles once and returns a reusable predicate; `matchesQuery` now delegates to it
(single-shot, unchanged behaviour). `App.svelte` compiles the matcher once per search change via a reactive
and filters with it. Behaviour is identical (parity test); the win is O(N) regex compiles → 1 per keystroke
on the perf-critical filter path. Headless, safe to land unattended. Files: `src/lib/search.ts`,
`src/lib/search.test.ts`, `src/App.svelte`. Advances epic CPE-688.
