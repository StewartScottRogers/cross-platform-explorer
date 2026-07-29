---
id: CPE-905
title: Provider-agnostic recursive walk + tree transfer (all backends)
type: refactor
component: Backend
priority: medium
tags: ready
epic: CPE-616
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
The cancellable recursive `walk` + bidirectional `download_tree`/`upload_tree` primitives (added to the
SFTP provider in CPE-684/903) only use `FileSystemProvider` trait methods — so they belong to **all**
backends, not just SFTP. Extracted them into a generic **`cpe_server::transfer`** module over
`&dyn FileSystemProvider`, so local disk, SFTP, and WebDAV share one implementation.

- `cpe_server::transfer::{walk, download_tree, upload_tree}` + `WalkEntry`. An empty `root` = the
  provider's root; the path join handles both the remote `/`-root and a relative/`FakeProvider` `""` root.
- `cpe-sftp`'s `walk`/`download_tree`/`upload_tree` now **delegate** to it (API + tests unchanged);
  re-exports `WalkEntry`.
- `cpe-webdav` gains the same capability via the generic module (a test walks/downloads/uploads over WebDAV).

## Acceptance Criteria
- [x] Generic `walk`/`download_tree`/`upload_tree` over the trait; cancellable; unreadable dirs skipped.
- [x] Tested purely against `FakeProvider` (4 tests) + over the **SFTP** transport (delegated tests) + over
      the **WebDAV** transport (a walk/download/upload round-trip).
- [x] No duplicated transfer logic; `cargo test` (server/sftp/webdav) + clippy `-D warnings` clean, 3-OS.

## Work Log
- 2026-07-22 — DRY refactor + capability parity: every provider now has cancellable recursive walk +
  remote⇄local tree copy from one place. The app-side transfer-manager UI/progress is the attended
  remainder (epic CPE-616).
