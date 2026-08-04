---
id: CPE-1294
title: "Streaming variant for the type-mismatch tree sweep"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
`find_type_mismatches` (`type_mismatch_scan.rs`) collects the whole tree's hits into a `Vec` before returning
— on a huge tree the UI waits. Refactor it to a shared flush-callback walker (per STREAMING.md / the
`listing.rs` pattern) so a future streaming command can paint incrementally. Headless; the command wiring is
a separate integration ticket (CPE-1299).

## Build
- In `crates/server/src/type_mismatch_scan.rs`: extract the DFS walk into a shared
  `fn walk_type_mismatches(root, mut flush: impl FnMut(Vec<MismatchHit>) -> ControlFlow<()>) -> ScanTail`
  mirroring `listing.rs::stream_dir_entries` (`flush` gets batches; return `Break` stops the walk early;
  return the `scanned`/`truncated` tail). Re-express the existing `find_type_mismatches(root) ->
  MismatchReport` as a thin collect-to-vec over the walker (behavior byte-identical — existing tests must
  still pass unchanged).
- Batch hits (e.g. flush every N or per-directory) so streaming is chunked, not per-item.
- Add in-crate tests mirroring `listing.rs`'s `stream_dir_entries_*`: batches are delivered, an early
  `ControlFlow::Break` from `flush` stops the walk, and the collect-to-vec wrapper equals the old output.
- **Do NOT wire the `ipc::Channel` command or touch `src-tauri`/bindings** — that is CPE-1299. Keep this
  disjoint from the other stream tickets. No new dep; never panic.

## Acceptance criteria
- The collect-to-vec `find_type_mismatches` is unchanged in behavior (all existing tests green); the new
  walker delivers batches and honors an early `Break`.
- `cargo test -p cpe-server` green; `cargo clippy` clean both feature modes; no new dep.

## Notes
Template: `crates/server/src/listing.rs` (`stream_dir_entries`). Streaming-liveness convention (STREAMING.md).
Epic CPE-1002; command wiring = CPE-1299 (shared with CPE-1295/1296).

## Work Log
