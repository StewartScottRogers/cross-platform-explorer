---
id: CPE-921
title: Search box needs a documentation button linking to search-options docs
type: feature
component: Frontend
priority: medium
tags: ready
created: 2026-07-23
status: Done
---

## Summary
Both search boxes (Find by name, Search in files) should carry a small "Docs" button in their header that
opens the in-app documentation straight to the search page (`12-search`, which documents the search options
— globs/wildcards, match-case, recents, truncation). Users tweaking search options should have one-click
access to what the options mean.

## Acceptance Criteria
- [x] ContentSearchDialog and FileNameSearchDialog headers each show a book/docs icon button.
- [x] Clicking it opens DocsView at the `12-search` page.
- [x] Matches the menu/button conventions (theme colours, book glyph, no hard-coded colours).
- [x] `npm run check` passes.

## Work Log
- 2026-07-23 — Filed + started.

- 2026-07-23 — Added a book (Docs) button to both search dialog headers (Find by name, Search in files); opens DocsView at `12-search`. Extended that doc with a content-search/Match-case section. Added `search.docsTitle` to all 12 complete locales. `npm run check` + i18n/docs guards green.
