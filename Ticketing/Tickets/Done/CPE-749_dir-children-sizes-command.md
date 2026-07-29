---
id: CPE-749
title: Backend dir_children_sizes — per-child recursive sizes for the space analyzer
type: feature
component: Backend
priority: medium
status: Open
tags: ready
created: 2026-07-19
epic: CPE-706
estimate: 2-3h
---

## Summary
Child of CPE-706 and its **foundation**. The treemap/drill-down and largest-files views need each
*immediate child's* recursive size, which the existing `dir_size` (one grand total) and `folder_stats`
(aggregate) don't give. Add `dir_children_sizes(path)` → a list of `{name, path, is_dir, size}` for the
direct children of `path`, each folder's `size` being its recursive total (reuse `dir_size`'s symlink-safe
walk), files their own length. Cancellable so a scan of a huge tree can be abandoned.

## Scope
- New `#[tauri::command] dir_children_sizes(path) -> Result<Vec<ChildSize>, String>`; reuse the `dir_size`
  walk per child directory (already symlink-cycle-safe, CPE-611).
- Register in `generate_handler!`; skip unreadable children (preserve `list_dir`'s skip-don't-fail rule).
- Cancellation: a cancel token / generation the frontend can bump to stop an in-flight scan (or a
  streamed variant per STREAMING.md if a folder has very many children — decide at build).
- cargo-tested against a temp tree (nested dirs + files); no exact-fs-size asserts beyond controlled
  content (3-OS matrix — assert relative ordering / sums of known bytes, not platform block sizes).

## Acceptance
- [ ] `dir_children_sizes` returns each direct child with its recursive size; folders sum their subtree,
  files their length; unreadable entries are skipped, not fatal.
- [ ] A long scan can be cancelled without crashing or leaking a thread.
- [ ] cargo-tested; clippy clean in both feature modes.

## Notes
Foundation for CPE-751 (treemap) and the largest-files surfacing. `dir_size` stays as-is for the single-
folder total used by CPE-750's column.
