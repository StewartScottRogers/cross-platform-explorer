---
id: CPE-1295
title: "Streaming variant for the dangling-symlinks sweep"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
`find_dangling_links` (`dangling_links_scan.rs`) collects all resolved dangling/cyclic link records into a
`Vec` before returning. Refactor to a shared flush-callback walker so a future streaming command paints
incrementally. Headless; command wiring is CPE-1299.

## Build
- In `crates/server/src/dangling_links_scan.rs`: extract the walk into
  `fn walk_dangling_links(root, mut flush: impl FnMut(Vec<DanglingLink>) -> ControlFlow<()>) -> ScanTail`
  mirroring `listing.rs::stream_dir_entries` — flush batches of classified links, honor an early `Break`,
  return the `scanned`/`truncated` tail. Re-express `find_dangling_links(root) -> DanglingReport` as a thin
  collect-to-vec over it (behavior unchanged; existing tests pass). Preserve the loop-safe lexical
  `normalize` (no `canonicalize`) and the `#[cfg(unix)]`-gated creation tests.
- Batch links per directory (or every N). In-crate batching + early-`Break` tests.
- **No `ipc::Channel` command / no `src-tauri`/bindings** — that is CPE-1299. Disjoint from the other stream
  tickets. No new dep; never panic.

## Acceptance criteria
- Collect-to-vec `find_dangling_links` behavior unchanged (existing tests green); walker delivers batches +
  honors `Break`.
- `cargo test -p cpe-server` green; `cargo clippy` clean both feature modes; no new dep.

## Notes
Template: `listing.rs`. Epic CPE-1002; command wiring CPE-1299. Independent of CPE-1294/1296.

## Work Log
- 2026-08-03 — dangling-links streaming walker merged (#594). Reviewer APPROVE + validated the design: is_cyclic follows the chain through the full by_path map, so per-directory incremental flush WOULD misclassify a cross-dir cycle (concrete example) — walk-to-completion-then-batch(256) is required for correctness, not a missed optimization. Parity structurally guaranteed, loop-safe normalize preserved, 12/12 re-run (unix tests on CI legs), clippy clean.
