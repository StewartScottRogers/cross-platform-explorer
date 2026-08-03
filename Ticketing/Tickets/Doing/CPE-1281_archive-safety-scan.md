---
id: CPE-1281
title: "Archive expansion-ratio scan (zip-bomb warning)"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
The pure `archive_safety` core (expansion-ratio math + `RatioLimits`) exists but has NO scan adapter and NO
command. Add a `cpe-server` adapter that opens a real archive, gathers per-entry compressed/uncompressed
sizes, and produces a zip-bomb `RatioReport`. Headless, cargo-tested.

## Build
- New module `crates/server/src/archive_safety_scan.rs` (declare it with `pub mod archive_safety_scan;` in
  `crates/server/src/lib.rs`). A pure `fn analyze_archive_safety(path: &Path) -> RatioReport` (or a result
  struct) that: opens the zip via the existing `zip` crate (mirror how `archive.rs` opens archives), iterates
  `by_index` collecting each entry's `(compressed_size(), size())` into `archive_safety::EntrySizes`, calls
  `archive_safety::expansion_ratio(&entries, &RatioLimits::default())`, and returns the resulting report
  (flagged entries + overall ratio + a `truncated` flag if an entry cap is hit). Skip/So tolerate unreadable
  entries rather than failing the whole scan.
- Add `#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]` (+ `Serialize`) to the result struct(s)
  returned to the frontend (`RatioReport`/`FlaggedEntry`/`RatioLimits` as needed) so a later command can expose
  them.
- **Do NOT wire the `#[tauri::command]` or regen bindings in this ticket** — that is the shared integration
  ticket (CPE-1287). This ticket delivers the pure, tested `cpe-server` adapter only, so it stays disjoint from
  the other Shift-1 modules.
- No new dependency (reuse `zip`). Never panics on a malformed archive.

## Acceptance criteria
- `analyze_archive_safety` returns a report flagging a high-ratio (zip-bomb-like) entry and not flagging a
  normal archive; unreadable/garbage input yields a graceful result, never a panic.
- `cargo test -p cpe-server` covers it with a crafted in-tempdir zip (a tiny highly-compressible entry →
  high ratio; a normal small archive → not flagged).
- `cargo clippy` clean in both feature modes; no new dep.

## Notes
Template: `crates/server/src/folder_similarity_scan.rs` (walk + caps + `truncated` + skip-unreadable). Epic
CPE-1002. Command wiring + bindings + the "File Health" UI are separate (CPE-1287 / attended).

## Work Log
