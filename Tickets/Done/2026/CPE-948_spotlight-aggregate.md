---
id: CPE-948
title: Spotlight multi-source result aggregation
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
Second headless slice of the spotlight overlay (CPE-704), building on `spotlight::rank` (CPE-937).
`cpe_server::spotlight_results`:
- `ResultKind { Action, Folder, File, Recent }` (declaration order = section priority) + `SpotResult` +
  `SpotSection`.
- `aggregate(query, sources, per_kind_cap) -> Vec<SpotSection>` — rank each source, cap per kind, return
  non-empty sections ordered by kind priority.
- `top(query, sources, total_cap) -> Vec<SpotResult>` — a flat best-first list across all sources, capped,
  tie-broken by kind then shorter text.

## Acceptance Criteria
- [x] Grouped sections capped + ordered; empty sections dropped; flat top() merged best-first + capped.
- [x] 4 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Second CPE-704 slice: aggregates fuzzy hits from files/folders/actions/recents.
