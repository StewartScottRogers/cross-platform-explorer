---
id: CPE-1302
title: "Exclude-glob support for the tree-scan walkers"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
The tree-walking safety scans (mismatch, dangling-links, orphan-sidecars, empty-dirs) walk EVERYTHING —
including `node_modules`/`.git`/`target`, which bloats results and slows the sweep. Add optional exclude-glob
support so a caller can skip matching directories/paths. This is GLOB-exclude (not `.gitignore` semantics —
there is no gitignore parser in the repo; scope it honestly as glob patterns).

## Build
- **Shared matcher:** a glob matcher is currently DUPLICATED privately as `glob_is_match` in `selection.rs`
  and `name_search.rs`. Promote ONE to a shared `pub fn` (e.g. `crate::glob::glob_match(pattern, name)` or
  reuse an existing module) and have both call sites use it — no behavior change to them (add a parity test).
- **Walkers:** thread an optional `excludes: &[String]` (empty = current behavior, so existing callers/tests
  are unaffected) through the tree-walk scan fns in `type_mismatch_scan.rs`, `dangling_links_scan.rs`,
  `orphan_sidecars_scan.rs`, `empty_dirs_scan.rs` (both the `walk_*` and the collect-to-vec wrappers). During
  the walk, skip a directory/entry whose base name (and optionally its path) matches ANY exclude glob — prune
  the directory (don't descend). Do NOT change `archive_safety_scan` (it scans inside an archive, not a tree).
- **Commands:** add the `excludes: Vec<String>` arg to the non-stream scan commands (`find_type_mismatches`,
  `find_dangling_links`, `find_orphan_sidecars`, `find_empty_dirs`) in `src-tauri/src/lib.rs` and pass it
  through; regen `src/lib/bindings.gen.ts` (drift guard). The `*_stream` commands MAY also take it, or note a
  tiny follow-up — your call, but keep bindings zero-drift either way.
- No new dependency. Never panic. Default (empty excludes) must be byte-identical to today.

## Acceptance criteria
- With an exclude like `["node_modules", ".git", "target"]`, the scans skip those directories (a hit inside
  an excluded dir is NOT reported and the dir isn't descended); with empty excludes, behavior is unchanged
  (all existing scan tests pass). The shared matcher is used by both former `glob_is_match` sites.
- `cargo test -p cpe-server` green (new exclude tests + a shared-matcher parity test); `npm run check` +
  `cargo build`/`clippy` (both feature modes) green; bindings zero-drift; no new dep.

## Notes
Final shift-3 headless item (PM: value thinning beyond this). Glob-exclude, NOT gitignore. Epic CPE-1002.
Touches the walkers refactored in CPE-1294/1295/1296 + empty_dirs + the shared matcher + commands + bindings.

## Work Log
