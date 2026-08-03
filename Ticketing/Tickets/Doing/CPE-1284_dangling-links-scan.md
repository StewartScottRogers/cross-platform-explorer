---
id: CPE-1284
title: "Dangling / cyclic symlink scan"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
The pure `dangling_links` core (`scan_dangling` classifying `Missing` / `Cyclic`) exists but has no
filesystem-walk adapter and no command. Add a `cpe-server` adapter that walks a tree, collects symlinks, and
classifies broken (target missing) and cyclic (self/loop) links. Headless, cargo-tested.

## Build
- New module `crates/server/src/dangling_links_scan.rs` (declare `pub mod dangling_links_scan;` in
  `crates/server/src/lib.rs`). A pure `fn find_dangling_links(root: &Path) -> DanglingReport` that walks
  `root` (skip-unreadable), detects symlinks via `fsutil::entry_is_symlink` + `std::fs::read_link`, builds
  `dangling_links::LinkEntry` records (with a resolved-target existence check), calls `scan_dangling(&links)`,
  and returns the classified links + `scanned` / `truncated`.
- `#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]` + `Serialize` on the result struct.
- **No command / no bindings here** — integration is CPE-1287. Pure adapter + tests only.
- Cross-platform: test the pure classification cross-platform; gate the actual symlink-CREATION test to
  `#[cfg(unix)]` (Windows symlink creation may need privilege). No new dep; never panics.

## Acceptance criteria
- A symlink to a deleted target classifies `Missing`; a self/loop symlink classifies `Cyclic`; a valid
  symlink is not flagged (fixture under `#[cfg(unix)]`).
- `cargo test -p cpe-server` covers it; `cargo clippy` clean both feature modes; no new dep.

## Notes
Template: `folder_similarity_scan.rs` + `fsutil.rs` symlink helpers. Epic CPE-1002. Command/UI separate.

## Work Log
