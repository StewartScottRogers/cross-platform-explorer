---
id: CPE-903
title: SFTP upload_tree (local→remote transfer) — symmetric to download_tree
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-616
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
Completes bidirectional transfers on the SFTP provider: `upload_tree(local_dir, remote_root, cancel)` —
the symmetric counterpart to CPE-684's `download_tree`. Recursively walks the local directory, `mkdir`s
remote directories and `write`s remote files, recreating the structure. Cancellable (`&AtomicBool`
checked before each entry). Windows source paths are mapped `\` → `/` so the remote gets POSIX paths.

## Acceptance Criteria
- [x] `upload_tree` recreates a local tree on the remote (dirs + file bytes), returning the file count.
- [x] Cancellable; unreadable local dirs skipped.
- [x] Verified against the fs-backed in-process server (upload a 2-file tree, read both back over SFTP).
- [x] `cargo test` (cpe-sftp 13) + clippy `-D warnings` clean, 3-OS.

## Work Log
- 2026-07-22 — With `download_tree` (CPE-684), the SFTP provider now has both transfer directions. The
  app-side transfer-manager UI + progress remain attended (epic CPE-616).
