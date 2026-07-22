---
id: CPE-866
title: Toolbar Search appears to do nothing — make it actually search
type: bug
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
User reports the **Search box on the main toolbar does nothing**. The intended wiring exists —
`NavToolbar` search `<input on:input>` dispatches `search` → `App` sets `search` → `ExplorerPane`'s
derivation filters `visible` by `makeMatcher(search)` (this filter path passes the `App.features`
wildcard-search test) — so the bug is either a runtime regression or an expectation gap.

## Clarified (user, 2026-07-21)
The current-folder filter works *inside a folder*; at **Home** it does nothing (no file list to filter),
which is what the user saw. The ask: make the toolbar Search work **like Windows File Explorer search** —
find files **recursively** (current folder + subfolders), support **wildcards + other query features**, and
ship **documentation** for the syntax.

## Plan
Reuse the existing recursive engine (`cpe_server::name_search`, streaming via `find_files_by_name_stream`,
CPE-603/666) which already supports substring, `*`/`?` globs, and `{a,b}` brace groups (case-insensitive).
- Keep the instant **as-you-type** current-folder filter (fast feedback in a folder).
- On **Enter** in the toolbar Search box, run a **recursive** search scoped to the current folder and show
  the hits (name + location, streamed) via the `FileNameSearchDialog`, pre-filled with the query.
- Add a **Search** docs page (`src/docs/`) documenting substring, `*`, `?`, `{a,b}`, case-insensitivity, and
  recursive-on-Enter; register it in the docs library.
- Follow-up (noted, not this ticket): fully-inline auto-recursive results + a Home/all-drives scope.

## Acceptance Criteria
- [x] Typing filters the current folder; **Enter** runs a recursive, wildcard-capable search showing results.
- [x] Documentation page covers the wildcard/query syntax and is in the in-app docs library.
- [x] `npm run check` + suite green; GUI-verified with the user driving the running app.

## Work Log
- 2026-07-21 — Filed; user clarified: want Windows-Explorer-style recursive search + wildcards + docs. Reusing
  the CPE-603 name-search engine; Enter escalates the toolbar box to a recursive search.
