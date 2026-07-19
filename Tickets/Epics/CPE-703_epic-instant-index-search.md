---
id: CPE-703
title: "EPIC: Instant index search (Everything-style)"
type: Task
status: Proposed
priority: High
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-18
closed:
---

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
