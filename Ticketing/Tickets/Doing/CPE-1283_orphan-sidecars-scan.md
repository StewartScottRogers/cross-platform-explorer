---
id: CPE-1283
title: "Orphaned-sidecar scan"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
The pure `orphan_sidecars` core (`find_orphans` + `default_rules`) exists but has no scan adapter and no
command. Add a `cpe-server` adapter that lists a folder's files and flags sidecar files whose primary is
gone (a `.srt`/`.xmp`/`.aae`/etc. with no matching media/primary file). Headless, cargo-tested.

## Build
- New module `crates/server/src/orphan_sidecars_scan.rs` (declare `pub mod orphan_sidecars_scan;` in
  `crates/server/src/lib.rs`). A pure `fn find_orphan_sidecars(root: &Path, recursive: bool) -> OrphanReport`
  that lists the folder's files (optionally recursive; skip-unreadable) into `orphan_sidecars::FileEntry`,
  calls `find_orphans(&entries, &default_rules())`, and returns the orphan sidecar paths + `scanned` /
  `truncated`.
- `#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]` + `Serialize` on the result struct.
- **No command / no bindings here** — integration is CPE-1287. Pure adapter + tests only.
- No new dep; never panics.

## Acceptance criteria
- A `.srt` whose `.mp4` primary EXISTS is NOT flagged; a `.xmp` whose primary is absent IS flagged, over a
  tempdir fixture.
- `cargo test -p cpe-server` covers both; `cargo clippy` clean both feature modes; no new dep.

## Notes
Template: `folder_similarity_scan.rs`. Epic CPE-1002. Command/UI separate.

## Work Log
