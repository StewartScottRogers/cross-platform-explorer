---
id: CPE-574
title: "Content search — collapse a file's matches"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
On a large "Search in files" result the list is a long scroll. Let you collapse a file's matches via a
chevron on its header, so you can fold away files while keeping the header as a jump-to link.

## Acceptance Criteria
- [x] A ▸/▾ chevron on each result-file header toggles its matches collapsed/expanded.
- [x] The file header still jumps to the file (chevron uses stopPropagation).
- [x] `npm run check` clean; a component test covers collapse.

## Resolution
`ContentSearchDialog`: a `collapsedFiles` `Set<string>` keyed by path; each group header is now a
`.group-head` with a `.chev` toggle (`stopPropagation`) beside the existing jump-to `.file` button, and
the matches render only `{#if !collapsedFiles.has(g.path)}`. `ContentSearchDialog.test.ts` +1 (search →
collapse first file → its match hidden, other file's still shown; via a `.code`-textContent matcher for
the highlighted lines). Full suite **614 pass / 63 files**; `npm run check` 0/0. Dialog is non-localized.
