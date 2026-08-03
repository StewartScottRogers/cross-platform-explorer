---
id: CPE-1282
title: "Empty-folder cascade scan"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
The pure `empty_dirs` core (`cascade_empty` over a `DirNode` tree) exists but has no filesystem-walk adapter
and no command. Add a `cpe-server` adapter that walks a real folder tree and returns the topmost
cascade-empty directories (a dir that is empty or contains only empty dirs). Headless, cargo-tested.

## Build
- New module `crates/server/src/empty_dirs_scan.rs` (declare `pub mod empty_dirs_scan;` in
  `crates/server/src/lib.rs`). A pure `fn find_empty_dirs(root: &Path) -> EmptyDirsReport` that recursively
  walks `root` (skip-unreadable), builds the `empty_dirs::DirNode` tree, calls `cascade_empty(&root_node)` to
  get the topmost cascade-empty dir paths, and returns them plus `scanned` / `truncated` counters (cap the
  walk like the `folder_similarity_scan` template).
- `#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]` + `Serialize` on the result struct.
- **No `#[tauri::command]` / no bindings regen here** — integration is CPE-1287. Pure adapter + tests only.
- No new dep; never panics.

## Acceptance criteria
- Returns the topmost cascade-empty directories for a tempdir tree (nested empty dirs collapse to their
  common ancestor; a branch containing any real file is NOT flagged, nor are its ancestors solely on its
  account).
- `cargo test -p cpe-server` covers the nested-empties + non-empty-branch cases; `cargo clippy` clean both
  feature modes; no new dep.

## Notes
Template: `folder_similarity_scan.rs`. Epic CPE-1002. Command/UI separate.

## Work Log
