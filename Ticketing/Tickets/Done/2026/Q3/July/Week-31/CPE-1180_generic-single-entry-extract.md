---
id: CPE-1180
title: "Backend: generic single-entry extract for non-zip archives (tar/tar.gz/7z)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-705
---

## Summary
Part of the CPE-705 GUI remainder. `extract_archive_entry` is **zip-only**; browsing INTO a tar/tar.gz/7z
archive and opening a leaf needs a generic single-entry extractor. Backend-only; unblocks CPE-1181's leaf-open.

## Build
- Add `extract_archive_entry_any(path, inner)` in `crates/server/src/archive.rs` dispatching by extension
  (tar / tar.gz / tgz / 7z) to extract one inner file to a temp path, mirroring the existing zip-only
  `extract_archive_entry`. Preserve the zip-slip / path-safety guard (`entry_name_is_safe`).
- Thin `#[tauri::command]` in `src-tauri/src/lib.rs`; **regenerate `src/lib/bindings.gen.ts`** (specta-drift
  guard — [[regen-specta-bindings-on-struct-change]]).

## Acceptance Criteria
- [x] `cargo test -p cpe-server`: build a fixture `.tar.gz` and `.7z` in-test (via the existing create fns),
      extract a known entry to temp, assert bytes equal the original; path-traversal entry rejected.
- [x] `cargo clippy --all-targets -D warnings` clean (both feature modes); bindings regenerated.

## Work Log
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-705). Backend-only, fully parallel with the frontend tickets.
- 2026-07-31 — Implemented `extract_archive_entry_any(path, inner)` in `crates/server/src/archive.rs`,
  dispatching by extension: `.tar`/`.tar.gz`/`.tgz` via a shared `extract_tar_entry` helper, `.7z` via a
  new `extract_7z_entry` (built on `sevenz_rust::decompress_file_with_extract_fn`, applying the same
  `entry_name_is_safe` guard to every archive-reported name as `extract_7z_safe`), and `.zip` delegating
  to the existing `extract_archive_entry`. `inner` is validated with `entry_name_is_safe` up front,
  before any format-specific extraction, so a traversal name is rejected regardless of archive type.
  Factored the shared "flat temp file at `%TEMP%/cpe-archive/<basename>`" target into
  `temp_extract_target`, reused by both extractors. Added a thin `#[tauri::command]`
  `extract_archive_entry_any` in `src-tauri/src/lib.rs` (mirrors `extract_archive_entry`'s
  `note_app_op` temp-path recording) and registered it in both `generate_handler!` and the
  `export_bindings` `collect_commands!` list; regenerated `src/lib/bindings.gen.ts`
  (`extractArchiveEntryAny` now present). New tests: tar.gz + 7z round trip (built via
  `compress_to_targz` and a `sevenz_rust::SevenZWriter`-based fixture), plain `.tar` and `.tgz`-extension
  round trip, zip delegation, missing-entry error, and traversal-`inner` rejection across both tgz and
  7z. `cargo test -p cpe-server`: 1105 passed (17 in `archive::tests`), 0 failed. `cargo clippy
  --all-targets -- -D warnings` clean for `cpe-server` (default + `--features index`) and for
  `src-tauri` (default + `--features sidecar-platform`). `cargo test` in `src-tauri`: 73 passed, 0
  failed. -> Done.
