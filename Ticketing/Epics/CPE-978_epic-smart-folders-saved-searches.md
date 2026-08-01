---
id: CPE-978
title: "EPIC: Smart folders & saved searches"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-24
closed: 2026-08-01
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

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** Sidebar smart-folder surface + live-refresh + persistence unbuilt (only pure saved-query evaluator).

## Re-activated 2026-08-01 (workshift) — DoD-gap assessment + real decomposition
Grep-first assessment (the 2026-07-30 "remaining" note was STALE both ways). TRUE state:
- `src/lib/savedSearch.ts` (CPE-986, this epic's OWN structured-query model + `evaluateSavedSearch`,
  9 tests) is built but ORPHANED — no store, no persistence, no UI, referenced only by its test.
- A SEPARATE tag-only smart folder (`src/lib/smartFolders.ts`, epic CPE-614/CPE-667) IS wired
  end-to-end (sidebar section, localStorage persist, rename/delete, reactive on `$tags`) — covers
  ~half the DoD but only a single-tag query, NOT this epic's structured search.
- DoD grade: sidebar-appears DONE; open-shows-matches DONE (tag scope only); persist DONE (tag store);
  save-a-structured-search MISSING; live-refresh-on-FS-change MISSING (only tag-change today);
  reorder MISSING.

Decomposition (all headless-buildable TS/Svelte; SEQUENTIAL — they overlap App.svelte/Sidebar.svelte):
- CPE-1228 — Saved-search store + persistence (foundation), wrapping serializeSavedSearch/parseSavedSearch.
- CPE-1229 — Wire structured smart folders end-to-end: "Save search…" + sidebar surface + open-evaluator.
- CPE-1230 — Live refresh on filesystem change (folder-watch/CPE-833), not just $tags.
- CPE-1231 — Reorder smart folders (both stores) + Sidebar reorder UI.
- Deferred: semantic saved query (composes with CPE-976 + a model key).

## Closed 2026-08-01 (workshift) — DoD met
Wired the orphaned structured saved-search model into a complete, live feature. All DoD elements met:
- **Save a search as a named smart folder** — CPE-1229 ("Save search…" in SelectByDialog + palette).
- **Opening shows current matches across the tree** — CPE-1229 open-evaluator (scans the captured root
  via `scan_tree`, filters through the existing `evaluateSavedSearch` — one matcher, no parallel logic).
- **Refreshed as files change** — CPE-1230 (reuses the existing folder-watch bus, debounced in-scope
  recompute, unsubscribe on exit; tag scope watches parent dirs so the watcher actually arms).
- **Persist across sessions** — CPE-1228 (`savedSearchStore.ts`, localStorage, tolerant parse).
- **Edit / reorder / remove** — rename+remove existed; CPE-1231 added reorder (both stores).
- **Plain explorer unaffected** — sidebar section gated on non-empty; no always-on cost.

**Children:** CPE-1228 (store) · CPE-1229 (Save + sidebar + open-evaluator) · CPE-1230 (live-refresh) ·
CPE-1231 (reorder) · CPE-1233 (gui-smoke pin) · CPE-1234 (preview-icon fix). Gauntlet: every child got
Reviewer + (behavioural) UAT; the marquee UI slice + preview fix got Visual Critic **VISUAL PASS**
(saved-search sidebar section + magnifying-glass preview placeholder). CPE-1230 bounced once (two
independent defects: a watchLive "off means off" bypass + tag-folder watcher-never-armed) and was
fixed + re-verified. Follow-ups filed: CPE-1235 (parentDir POSIX-root edge). Deferred: semantic saved
query (composes with CPE-976 + a model key).
