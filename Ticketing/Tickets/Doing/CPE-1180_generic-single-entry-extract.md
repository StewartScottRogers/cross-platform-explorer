---
id: CPE-1180
title: "Backend: generic single-entry extract for non-zip archives (tar/tar.gz/7z)"
type: feature
component: Backend
priority: medium
status: Doing
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
- [ ] `cargo test -p cpe-server`: build a fixture `.tar.gz` and `.7z` in-test (via the existing create fns),
      extract a known entry to temp, assert bytes equal the original; path-traversal entry rejected.
- [ ] `cargo clippy --all-targets -D warnings` clean (both feature modes); bindings regenerated.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-705). Backend-only, fully parallel with the frontend tickets.
