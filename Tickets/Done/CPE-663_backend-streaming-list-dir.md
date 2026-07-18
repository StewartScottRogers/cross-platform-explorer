---
id: CPE-663
title: Backend streaming list_dir over an ipc::Channel
type: feature
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-662
estimate: 1-2h
---

## Summary
First child of CPE-662 (streaming liveness). Add `list_dir_stream(path, on_entry)` that walks the
directory and pushes `Vec<DirEntry>` batches over a Tauri v2 `ipc::Channel` as it reads, instead of
returning the whole listing at once. Refactor the entry-mapping + read loop into a shared
`stream_dir_entries` walker so the existing synchronous `list_dir` and the stream can't diverge, and the
skip-unreadable contract is preserved in one place.

## Acceptance Criteria
- [x] `stream_dir_entries(path, batch, flush)` walks the dir, emits batches of ≤`batch` entries, skips
      unreadable entries, and returns the total emitted.
- [x] `list_dir` is reimplemented on top of the shared walker (identical output to before).
- [x] `#[tauri::command] list_dir_stream(path, on_entry: ipc::Channel<Vec<DirEntry>>)` sends batches
      (256/flush) and is registered in `generate_handler!`.
- [x] cargo tests: batching + all-entries-flushed + streamed contents equal `list_dir` contents.
- [x] clippy clean both feature modes; existing `list_dir` tests still pass.

## Work Log
2026-07-18 (nightshift) — Picked up as first CPE-662 child. Estimate 1-2h.

## Resolution
Extracted the directory read loop into `stream_dir_entries(path, batch, flush)` plus a `dir_entry_from`
mapper (src-tauri/src/lib.rs); `list_dir` now collects through the same walker, so the synchronous and
streaming paths share one skip-unreadable/mapping implementation. Added `#[tauri::command]
list_dir_stream(path, on_entry: ipc::Channel<Vec<DirEntry>>)` that flushes 256-entry batches over the
channel and returns the total count, registered in generate_handler!. Two cargo tests cover batching/
all-flushed and streamed-equals-list_dir contents (118 backend tests pass; clippy clean both feature
modes). Frontend consumption is the next child (CPE-664). Files: src-tauri/src/lib.rs.
