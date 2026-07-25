---
id: CPE-937
title: Spotlight fuzzy match + ranking core
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-704
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
First headless slice of the global quick-launch spotlight (CPE-704). `cpe_server::spotlight`:
- `fuzzy_score(query, candidate) -> Option<(score, positions)>` — case-insensitive subsequence match with
  the usual affordances: prefix + word-boundary (separators and camelCase humps) + consecutive-run
  bonuses, leading/inner gap penalties, and a mild shorter-is-better tie-break. `None` for non-matches;
  returns the matched char indices for UI highlighting. Empty query matches everything (score 0).
- `rank(query, items) -> Vec<SpotlightMatch{text,score,positions}>` — score + drop non-matches, sorted
  best-first with a stable length-then-original tie-break; blank query returns all in order.

The overlay UI feeds it candidate strings (file/folder/action labels) and renders the ranked hits.

## Acceptance Criteria
- [x] Subsequence matching (case-insensitive) with prefix/word-start/consecutive scoring; non-matches drop.
- [x] Stable ranking; empty query = all in order. 8 unit tests; clippy `-D warnings` clean.

## Work Log
- 2026-07-23 (dayshift) — Activated CPE-704 with the fuzzy scorer + ranker. The system hotkey, the overlay
  window, and wiring real files/actions/folders into it are the remaining children.
