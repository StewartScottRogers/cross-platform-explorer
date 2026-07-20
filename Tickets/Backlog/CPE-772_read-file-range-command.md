---
id: CPE-772
title: Backend streaming byte-range read command (read_file_range)
type: feature
status: Open
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
- [ ] Reads the exact requested range; clamps at EOF; errors cleanly on missing/denied files.
- [ ] Async (never blocks the main thread); cargo-tested (range math + clamping) on the 3-OS CI matrix.
- [ ] Wired through `src/lib/invoke.ts`.

## Notes
Prereq for CPE-773. Backend/CI-verified (no local-only fs byte-count assertions — cross-OS matrix).
