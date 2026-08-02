---
id: CPE-1265
title: "Content-search UI: bump generation token on rebuild + cover probe reject branch"
type: chore
component: frontend
priority: low
status: Backlog
tags: ready
created: 2026-08-02
epic: CPE-976
---

## Summary
Two non-blocking robustness items from the CPE-1263 review (PR #562), both narrow edge cases:
1. `ContentIndexSearchDialog.svelte` `buildIndex()` ("Rebuild index") does NOT bump the `gen` generation token, so a
   `content_search` already in flight when a rebuild is triggered could overwrite fresh post-rebuild state with
   pre-rebuild results after it resolves. Requires clicking Rebuild mid-debounced-search — very narrow.
2. `probe()`'s reject branch (the opening `content_search`/index-existence probe failing at the IPC layer, vs a
   resolved `index_exists:false`) has no dedicated test.

## Build
- Bump `gen` (or otherwise invalidate in-flight searches) at the start of `buildIndex()` so stale pre-rebuild results
  can't render after a rebuild.
- Add a jsdom test for `probe()`'s reject branch (IPC failure → graceful state, not an unhandled rejection).

## Acceptance criteria
- A search in flight when Rebuild is clicked cannot overwrite post-rebuild state (test it).
- probe() IPC-failure path is covered + degrades gracefully.
- `npm run check` clean; existing ContentIndexSearchDialog tests still pass.

## Notes
Low priority. Both flagged non-blocking by the CPE-1263 reviewer.
