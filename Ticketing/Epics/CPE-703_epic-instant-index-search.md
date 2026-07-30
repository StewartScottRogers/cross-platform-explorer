---
id: CPE-703
title: "EPIC: Instant index search (Everything-style)"
type: Task
status: Done
priority: High
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-18
closed:
---

> **Activated 2026-07-21** (dayshift, autonomous — best-guess decisions logged, user delegated PM as with
> CPE-706/709/719). Started with the **backend-agnostic** slice (query grammar + ranking, CPE-831) that is
> reused by *any* index design, so it commits to no architecture. The genuinely big-design pieces — the
> index engine (backend choice) and the live change-watcher — are held as their own children (below) for a
> later slice, since the index-engine tradeoff and the Windows USN-journal privilege model benefit from
> attended review before building.

## Decisions (activated 2026-07-21 — autonomous best-guess, PM delegated)
- **First slice = the query core**, not the index. The query grammar + ranked-result model (CPE-831) is
  pure, needs no volume/privilege, and is reused whatever the index backend turns out to be — zero rework
  risk. Everything storage/crawl/watch waits behind it.
- **Index backend (leaning, not yet built):** *roll our own* compact in-memory + on-disk filename index
  rather than pull tantivy/sqlite-FTS, to honour the fast/small-when-off tiebreaker (a full-text engine is
  heavy for a filename-only index). Deferred to CPE-832 for an attended design confirmation before build.
- **Change-watching:** reuse the already-vendored `notify` crate (inotify/FSEvents/ReadDirectoryChangesW)
  as the portable baseline; the NTFS USN journal is a later Windows-only fast-path where privileges allow,
  with the watcher as the always-available fallback. Deferred to CPE-833.
- **Delete-test / fast-when-off:** the whole indexer is opt-in and gated so the plain explorer runs no
  indexer and pays zero startup/memory cost when the mode is off (the epic's hard DoD).

## Goal
A background-maintained filename index across every mounted volume that returns matches as you type —
sub-100ms, independent of the current folder — so a user can jump to any file on any drive instantly.

## Why
Today's wildcard/type-ahead search only scans the folder you're in. Power users expect an "Everything"-style
whole-system instant find. Delivered as a delete-testable mode, the core explorer stays small/fast when off.

## Rough scope (areas, not child tickets)
- A Rust indexer service: initial crawl + a persistent on-disk index (per volume).
- Incremental change-watching to keep the index live (NTFS USN journal / inotify / FSEvents).
- A query grammar (substring, wildcard, path/extension filters) and a ranked result model.
- A global search overlay in the frontend, streamed results, keyboard-first.

## Open questions (resolve at activation)
- Index backend: roll our own vs. an embedded engine (e.g. tantivy / sqlite FTS)? Size vs. speed tradeoff.
- USN-journal access needs privileges on Windows — fall back to a watcher when unavailable?
- Index staleness/rebuild policy and disk footprint budget; must honour the fast-when-off rule.

## Definition of Done
- Typing a query returns cross-volume filename matches in <100ms on a warm index.
- The index stays current as files are created/renamed/deleted, without a manual rescan.
- With the mode disabled, no indexer runs and there is zero measurable startup/memory cost.

## Child tickets
1. **CPE-831** — Query grammar + ranked-result model (`cpe-server::index_query`): parse `ext:`/`path:`
   filters + name terms (substring/glob/brace, reusing `name_search`), a `matches` predicate, and a
   relevance `score`/`rank`. Pure, backend-agnostic, cargo-tested. *Headless — buildable now.*
2. **CPE-832** — The index engine: a compact filename index (initial crawl + persistent on-disk store,
   per volume) queried via CPE-831. **Big-design — confirm the roll-our-own-vs-embedded backend choice
   attended before building.** *(prereq: 831)*
3. **CPE-833** — Live incremental change-watching (notify baseline; NTFS USN-journal fast-path where
   privileged) to keep the index current without a rescan. *(prereq: 832)*
4. **CPE-834** — Global search overlay (frontend): keyboard-first, streamed results, cross-volume.
   **GUI-verified — attended.** *(prereq: 831, 832)*


## Closed 2026-07-29 (attended, user go-ahead + GUI-verified)
The engine slices (CPE-831 query core / CPE-832 roll-our-own index engine / CPE-833 incremental primitives)
were already built + cargo-tested but wired to nothing. This session's enablement wave exposed them and shipped
the UI, all through the full Reviewer+UAT gauntlet with 3-OS CI green:
- **CPE-1137** (PR #452) — `IndexService` state + streamed `index_build`/`index_search`/`index_status`/`index_drop`/`index_clear` commands + persistence (`<app_data>/index/<volume>.idx`), off-means-off. (Reviewer caught + fixed a same-volume build race.)
- **CPE-1138** (PR #453) — live `notify` watcher feeding `apply_create/remove/rename`; pure `resolve_touched` mapping. (Reviewer caught + fixed a nested-create ordering bug.)
- **CPE-1139** (PR #454) — keyboard-first **Ctrl+K** global search overlay: streamed ranked cross-folder matches as you type, ↑/↓ + Enter to reveal, "Build index" opt-in affordance.

**DoD met + user-GUI-verified 2026-07-29:** Ctrl+K opens; Build index crawls the drive; typing returns
ranked cross-folder matches; a freshly created/renamed file reflects without a rebuild; with the mode unused,
no indexer runs (off-means-off). Deferred as a documented later optimisation: NTFS USN-journal fast-path; an
optional multi-drive picker; a gui-smoke render pin for the overlay (QA follow-up).
