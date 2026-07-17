---
id: CPE-577
title: "Content search — filter the result files by name"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
A broad "Search in files" can return many files. Add a client-side **Filter files** box that narrows the
shown result groups by filename/path — no re-search — so you can zero in on the files you care about.

## Acceptance Criteria
- [x] A filter box appears when a search returned more than one file; typing narrows the shown groups by
      path (case-insensitive substring); empty shows all.
- [x] The summary shows an `· N shown` count while filtering; a "no files match" hint when none do.
- [x] `npm run check` clean; a component test covers the filter.

## Resolution
`ContentSearchDialog`: `resultFilter` + `$: shownGroups = groups.filter(path.includes(...))`; the results
render `shownGroups`, with a `.result-filter` input (shown when >1 file), an `· N shown` summary annotation,
and a "No files match …" hint when the filter excludes all. `ContentSearchDialog.test.ts` +1 (search →
filter to `b.md` → other file hidden, matched file shown). Full suite **617 pass / 63 files**;
`npm run check` 0/0. Non-localized dialog.
