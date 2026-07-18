---
id: CPE-663
title: Backend streaming list_dir over an ipc::Channel
type: feature
component: Backend
priority: high
status: Open
tags: ready
created: 2026-07-18
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
- [ ] `stream_dir_entries(path, batch, flush)` walks the dir, emits batches of ≤`batch` entries, skips
      unreadable entries, and returns the total emitted.
- [ ] `list_dir` is reimplemented on top of the shared walker (identical output to before).
- [ ] `#[tauri::command] list_dir_stream(path, on_entry: ipc::Channel<Vec<DirEntry>>)` sends batches
      (256/flush) and is registered in `generate_handler!`.
- [ ] cargo tests: batching + all-entries-flushed + streamed contents equal `list_dir` contents.
- [ ] clippy clean both feature modes; existing `list_dir` tests still pass.

## Work Log
