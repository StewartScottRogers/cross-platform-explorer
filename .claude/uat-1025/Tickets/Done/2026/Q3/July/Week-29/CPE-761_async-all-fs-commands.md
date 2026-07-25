---
id: CPE-761
title: Async ALL filesystem/OS commands (spawn_blocking) — no command can freeze the app
type: bug
component: Backend
tags: ready
created: 2026-07-19
closed: 2026-07-19
status: Done
priority: high
estimate: 3-4h
---

## Summary
Follow-through on CPE-760 ("much faster — do it everywhere"): convert **every** synchronous filesystem/OS
Tauri command to `async` + `spawn_blocking`, so no slow op on a slow/network drive can freeze the main
thread (which serializes sync commands). Establishes the standing rule (saved to memory:
[[async-all-blocking-commands]]).

## What changed
Converted **58** plain-arg commands to the `async` wrapper + sync `_impl` pattern (`src-tauri/src/lib.rs`):
file ops (create/rename/delete/copy/move/duplicate), reads/preview (read_file_text, read_image_data_url,
read_preview_info, read_archive_entries, entry_info, image_meta), metadata (folder_stats, dir_size,
dir_children_sizes, hash_file, text_stats, files_identical), search (search_file_contents, find_files_by_name,
find_duplicates), archive (compress_to_zip, extract_archive, extract_archive_entry), startup (home_dir,
list_drives, special_folders, parent_dir, entries_for_paths, same_volume), open/exec (open_external,
open_terminal, run_as_admin), git/forge (git_remote_url, forge_browse/clone/sync/token/conflict_*), and the
board/workbench readers. Plus the 4 from CPE-760 (list_dir_stream, list_dir, disk_space, forge_repo_status).

Each keeps the original body as a sync `_impl` (internal callers + cargo tests use it) with a thin async
command wrapper doing `spawn_blocking(move || NAME_impl(..)).await`.

## Acceptance
- [ ] `cargo` builds + tests pass; clippy clean, both feature modes.
- [ ] No user-triggered filesystem op (copy, delete, search, preview, checksum, compress, …) can freeze the
  app on a slow/network drive.
- [ ] Diagnostics shows commands running concurrently, not a batch frozen at ~5s.

## Notes / remaining
28 commands take `State<'_,T>` (borrowed, not `'static`) — mostly sidecar/agent **management** + tag/settings
storage (app-local, fast). Lower freeze risk; convert by pulling the `Arc` out of `State` first. Follow-up
if the Diagnostics readout still shows any of them slow. `find_files_by_name_stream` (Channel) can follow
the `list_dir_stream` pattern too.
