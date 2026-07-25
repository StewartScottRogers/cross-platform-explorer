---
id: CPE-1027
title: Broken-symlink repair suggestion (pure-ish model)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-715
created: 2026-07-25
status: Backlog
---

## Summary
Link Forge (CPE-715) slice: when a symlink is broken (its target no longer resolves), suggest a likely
replacement by finding a file/dir with the same basename under given search roots — the logic behind a
future "Repair link…" action. Add `suggest_repair` to the EXISTING module `crates/server/src/links.rs`
(no new module, no `lib.rs` change).

`suggest_repair(broken_link: &str, search_roots: &[&str]) -> Option<String>`:
- Read the link's stored target via `std::fs::read_link`; take its **basename**. If the link isn't a
  symlink or has no readable target, return `None`.
- Search each root (bounded-depth recursive walk, e.g. depth ≤ 4, skipping unreadable dirs like `list_dir`)
  for an entry whose file name equals that basename. Return the first match's full path as `Some(String)`,
  in root order; `None` if nothing matches.
- Never follow into the broken link itself; don't panic on unreadable entries.

## Acceptance Criteria
- [ ] Returns the path of a same-named file found under a search root; `None` when no match.
- [ ] Non-symlink input and an intact (non-broken) link with no better match are handled sanely (documented
      behaviour); unreadable dirs are skipped, not fatal.
- [ ] Pure `std`/`walkdir`-free (reuse whatever recursive walk helper `cpe-server` already has if one fits —
      grep first, e.g. in `listing.rs`/`scan.rs`; else a small bounded `read_dir` recursion). No new deps.
- [ ] clippy clean both feature modes; ≥4 unit tests using a tempdir tree (broken link + same-named file
      elsewhere → found; no match → None; non-symlink → None).

## Notes
Do NOT touch `crates/server/src/volume.rs` or `lib.rs` (a sibling worker owns those). Only `links.rs` + this
ticket's Work Log. Keep the `list_dir` skip-on-error convention.

## Work Log
- 2026-07-25: Added `suggest_repair` + a private `find_by_name` helper to `crates/server/src/links.rs`.
  Grepped `crates/server/src` for an existing bounded recursive-walk helper first (`name_search.rs`'s
  `walk_name_matches`, `compare.rs`'s `scan_children`, `snapshot_capture.rs`'s `scan_walk`) — none was a
  clean fit: `walk_name_matches` is batching/streaming-shaped and collects *all* matches across one root
  rather than short-circuiting on the first match across *multiple* ordered roots, and `scan_children`
  builds a full tree rather than searching. Hand-rolled a small depth-capped (`REPAIR_SEARCH_MAX_DEPTH =
  4`) `read_dir` recursion instead, following the same skip-unreadable-dir/never-panic shape as those
  helpers (and `list_dir`). `DirEntry::file_type()` doesn't follow symlinks, so a symlinked "directory"
  reports `is_dir() == false` and the walk naturally never descends into a symlink (covers "don't recurse
  into the broken link" for free). Added 5 unit tests (found-under-nested-root, root-order/first-match,
  no-match, non-symlink-input, unreadable/missing-root) alongside the existing 3 in `links.rs`, all
  tolerating ungated Windows symlink creation like the pre-existing tests. `cargo test -q links` → 8/8
  `links.rs` tests pass (report doesn't separate cleanly from `dangling_links` under substring filtering;
  confirmed via `--list`). `cargo clippy --all-targets -- -D warnings` and `--all-features` variant both
  clean.
