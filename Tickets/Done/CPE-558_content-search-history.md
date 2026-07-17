---
id: CPE-558
title: "Content search — remember recent queries (autocomplete history)"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
estimate: 45m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Re-running a "Search in files" query means retyping it every time. Remember recent queries and offer them
as autocomplete on the search box (a native `<datalist>`), newest-first, so common searches are one click
away. Persisted locally like the other prefs.

## Acceptance Criteria
- [x] A pure `pushRecentSearch(list, query, max)` in `contentSearch.ts` returns a newest-first,
      de-duplicated, capped list; a blank query is ignored. Unit-tested.
- [x] `ContentSearchDialog` records each executed search and offers the recents as a `<datalist>`
      autocomplete on the query input; persisted in localStorage.
- [x] `npm run check` clean; unit tests cover the history helper.

## Resolution
Added pure `contentSearch.ts::pushRecentSearch(list, query, max=10)` — newest-first, exact-dedup (a repeat
moves to front), capped, blank ignored, trimmed (4 unit tests). `ContentSearchDialog` loads recents from
`localStorage` (`cpe.contentSearchRecents`, defensively parsed), records each executed query via
`pushRecentSearch` + persists, and offers them through a native `<datalist id="cs-recents">` bound to the
query input (`list=`). `contentSearch.test.ts` + `ContentSearchDialog.test.ts` 14 passed; `npm run check`
0/0. Purely additive; no backend change.

## Notes
Pure list helper keeps it testable; the dialog persists + renders. Purely additive. Complements CPE-557
(match highlighting) on the content-search thread.
