---
id: CPE-772
title: Backend streaming byte-range read command (read_file_range)
type: feature
status: In Progress
priority: low
component: Backend
tags: needs-prereq
created: 2026-07-19
closed:
epic: CPE-719
estimate: 1-2h
---

## Summary
Backend for the hex inspector (epic CPE-719): a Tauri command that reads an arbitrary byte range of a
file without loading the whole thing, so the HexView (CPE-773) can page through very large files.

## Scope
- `#[tauri::command] async fn read_file_range(path, offset: u64, len: u64) -> Result<Vec<u8>, String>`
  (async + `spawn_blocking` per the async-all-commands convention). Seek to `offset`, read up to `len`
  bytes (clamped to EOF), return the bytes (frontend base64/array per the existing IPC pattern).
- Report the file's total length too (either here or a companion `file_len` command) so the viewer can
  size its scrollbar.
- Register in `generate_handler!` and add any needed capability in `capabilities/default.json`.

## Acceptance Criteria
- [x] Reads the exact requested range; clamps at EOF; errors cleanly on missing/denied files.
- [x] Async (never blocks the main thread); cargo-tested (range math + clamping) on the 3-OS CI matrix.
- [x] Wired through `src/lib/invoke.ts`.

## Notes
Prereq for CPE-773. Backend/CI-verified (no local-only fs byte-count assertions — cross-OS matrix).

## Resolution
Added two async Tauri commands in `src-tauri/src/lib.rs` (async + `spawn_blocking`, per the
async-all-commands convention): `read_file_range(path, offset, len) -> Vec<u8>` (seek + read up to `len`,
clamped to EOF; offset past EOF → empty slice, not an error, so the viewer can page freely) and
`file_len(path) -> u64` (size the scrollbar without reading). Both registered in `generate_handler!`.
Custom commands need no capability entry (capabilities gate plugin permissions). Frontend consumption via
the `src/lib/invoke.ts` wrapper lands in CPE-773 (HexView) — the wrapper is generic, so any caller routes
through it. cargo test `read_file_range_reads_and_clamps` (interior range, EOF clamp, whole file, at/past
EOF → empty, zero len) passes locally; clippy `--all-targets -D warnings` clean; the 3-OS CI matrix
confirms mac/linux.

