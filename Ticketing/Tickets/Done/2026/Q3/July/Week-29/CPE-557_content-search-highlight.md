---
id: CPE-557
title: "Content search — highlight the matched text in result lines"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 45m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The "Search in files" results (CPE-417) show each matching line as raw text, so you have to eyeball where
the match actually is. Highlight the matched substring(s) within each result line, respecting the dialog's
Match-case toggle — the standard grep-results experience.

## Acceptance Criteria
- [x] A pure `highlightSegments(line, query, caseSensitive)` in `contentSearch.ts` splits a line into
      matched/unmatched segments (substring, non-overlapping, left-to-right); blank query → the whole line
      as one unmatched segment. Unit-tested (incl. case-insensitive + repeated matches + no match).
- [x] `ContentSearchDialog` renders match segments in `<mark>`, using the query + case setting that were
      actually searched (not a later-edited query).
- [x] `npm run check` clean; unit tests cover the highlighter.

## Resolution
Added pure `contentSearch.ts::highlightSegments(line, query, caseSensitive)` → `Segment[]`
(`{text, match}`), non-overlapping left-to-right substring split; blank query → whole line as one plain
segment (6 unit tests: single/all-occurrence, case-insensitive default, case-sensitive, blank, no-match,
start+end). `ContentSearchDialog` captures the `searchedQuery`/`searchedCase` at search time (so
highlighting matches the results, not a later edit) and renders each result line as segments, wrapping
matches in `<mark class="hl">` styled with theme `--accent` (not browser-default yellow). Updated the
existing dialog test for the now-split line text + asserted one `<mark>needle</mark>` per result line.
`contentSearch.test.ts` + `ContentSearchDialog.test.ts` 10 passed; `npm run check` 0/0. No backend change.

## Notes
Pure highlighter keeps it testable headlessly; the dialog just maps segments → `<mark>`/text. Purely
additive; no backend change (the match logic already ran server-side to find the lines).
