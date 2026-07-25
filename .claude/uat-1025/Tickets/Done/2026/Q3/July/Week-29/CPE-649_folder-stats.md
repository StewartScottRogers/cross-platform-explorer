---
id: CPE-649
title: Recursive folder statistics (files/folders/bytes) for Properties
type: feature
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
---

## Summary
A `folder_stats(path)` command returning the recursive file count, sub-folder count, and total bytes
under a folder — richer than `dir_size` (bytes only) for the Properties dialog. Cycle-safe (skips
symlinked dirs) and bounded (500k-entry cap, reports `truncated`).

## Acceptance Criteria
- [x] `folder_stats` walks the tree with an explicit stack, counts files/dirs + sums bytes, skips
      symlinked dirs, caps entries. Non-folder ⇒ error.
- [x] cargo test (counts + bytes + non-folder error) + clippy clean.

## Work Log
2026-07-18 (dayshift) — Backend stats command; a Properties-dialog wiring can consume it later.
