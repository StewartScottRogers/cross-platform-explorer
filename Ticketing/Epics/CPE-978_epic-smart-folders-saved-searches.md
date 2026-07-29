---
id: CPE-978
title: "EPIC: Smart folders & saved searches"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-24
closed:
---

> **Activated 2026-07-24** (workshift, Foreman — user away, decisions logged). First slice = the **pure
> saved-query model + evaluator** (CPE-986), in **TypeScript** (`src/lib/`) reusing the existing
> `Condition` matcher (`selectMatch.ts`/`colorRules.ts`) — NOT a parallel Rust matcher. Live-refresh wiring,
> the sidebar UI, and persistence follow; a semantic saved query composes with CPE-976 later.

## Child tickets
1. **CPE-986** — Pure saved-query model + evaluator (`src/lib/savedSearch.ts`): a serialisable `SavedSearch`
   (named set of `Condition`s + scope/sort) + `evaluateSavedSearch(entries, search, now)` reusing
   `matchesCondition`. Vitest. *Headless (TS) — buildable now.*
2. **CPE-988+** — Live refresh on change (reuse folder-watch/CPE-833 signals), the sidebar smart-folder
   surface + "Save search…", and persistence. **GUI.**

## Goal
Save a search as a **live virtual folder**: define it once (filters — name/glob, type, size, date, tag,
location; and later a semantic query) and it appears in the sidebar, always showing the current matching
files across the tree, updating as files change. macOS Smart Folders / Outlook Search Folders for this
explorer — plus one-click "save this search" from any search you just ran.

## Why
Power users repeat the same searches ("all invoices this quarter", "screenshots over 2 MB", "everything I
touched today"). Turning a search into a durable, self-updating folder removes that repetition and gives a
task-oriented way to view files that cuts across the physical tree. The matching machinery already exists —
the `Condition` model (CPE-774, reused by CPE-711 selection), `name_search`, and the index (CPE-703) — so
this is mostly a **saved-query model + evaluator + sidebar surface**, not new search tech. It's also the
natural home for a saved **semantic** query ([[CPE-976]]).

## Rough scope (areas, not child tickets)
- A **saved-query model**: a named, serialisable query = a set of `Condition`s (+ scope + sort) and,
  optionally, a semantic query string; persisted (per-user, secret-free JSON like `connections`/macro
  library).
- A **pure evaluator**: `evaluate(query, entries|index) -> matches`, reusing `matchesCondition`/`name_search`
  and, when semantic, [[CPE-976]] — one matcher, no parallel implementation.
- **Live refresh**: recompute on directory change (reuse CPE-833 signals / folder watch) so a smart folder
  is always current; streamed results (STREAMING.md) for big result sets.
- A **sidebar surface**: smart folders as places; a "Save search…" affordance from the search bar; edit/
  reorder/delete.

## Open questions (resolve at activation)
- Scope model: whole-index vs. a chosen root subtree per smart folder (perf + privilege).
- Refresh strategy: eager on change vs. lazy on open vs. hybrid; result caps + streaming.
- Storage location + sync with the existing places/bookmarks; export/import to share a smart folder.
- Precedence when a smart folder mixes structured `Condition`s and a semantic query.

## Definition of Done
- A user can save a search (structured, and later semantic) as a named smart folder in the sidebar.
- Opening it shows the current matches across the tree, refreshed as files change — no manual re-run.
- Smart folders persist across sessions and can be edited/reordered/removed; the plain explorer is unaffected
  when none are defined.

## Notes
- Reuses [[CPE-711]]'s `Condition` matcher and [[CPE-703]]'s index; composes with [[CPE-976]] (semantic) and
  [[CPE-737]]/tags. Build the **pure saved-query model + evaluator** first (headless, cargo-tested), then the
  sidebar UI + persistence. See [[prefer-streaming-liveness]], [[maintain-in-app-docs-library]].
