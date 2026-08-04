---
id: CPE-1296
title: "Streaming variant for the orphan-sidecars sweep"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
`find_orphan_sidecars` (`orphan_sidecars_scan.rs`) groups files per directory then collects all orphans into
a `Vec`. It streams naturally at the per-directory boundary — refactor to a flush-callback walker that emits
each directory's orphans as found. Headless; command wiring is CPE-1299.

## Build
- In `crates/server/src/orphan_sidecars_scan.rs`: extract the walk into
  `fn walk_orphan_sidecars(root, recursive, mut flush: impl FnMut(Vec<String>) -> ControlFlow<()>) ->
  ScanTail` — after computing a directory's orphans via `orphan_sidecars::find_orphans`, flush that
  directory's batch; honor an early `Break`; return the `scanned`/`truncated` tail. Re-express
  `find_orphan_sidecars(root, recursive) -> OrphanSidecarResult` as a thin collect-to-vec over it (behavior
  unchanged; existing tests pass, including the cross-directory no-false-match guarantee). Preserve the
  `recursive` flag.
- In-crate batching + early-`Break` tests.
- **No command / no `src-tauri`/bindings** — CPE-1299 does that. Disjoint from the other stream tickets. No
  new dep; never panic.

## Acceptance criteria
- Collect-to-vec `find_orphan_sidecars` behavior unchanged (existing tests green, incl. cross-dir guarantee);
  walker flushes per-directory batches + honors `Break`.
- `cargo test -p cpe-server` green; `cargo clippy` clean both feature modes; no new dep.

## Notes
Template: `listing.rs`. Epic CPE-1002; command wiring CPE-1299. Independent of CPE-1294/1295.

## Work Log
