---
id: CPE-902
title: Filesystem-backed SFTP test server + provider write create-or-overwrite fix
type: feature
component: Backend
priority: medium
tags: ready
epic: CPE-616
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
Replaced the canned in-process SFTP test server (read-only stub) with a **real filesystem-backed** one
(rooted at a per-server temp dir, mapping SFTP ops onto `std::fs`), so the provider's *full* surface —
list / stat / read **and** write / mkdir / delete — round-trips against actual files over a real SSH
handshake. Reads/writes are offset-based (open+seek+op per call), so no open-file table is needed.

Found + fixed a **real provider bug** along the way: `SftpProvider::write` used russh-sftp's convenience
`SftpSession::write`, which opens the remote file **WRITE-only** — so writing a *new* file failed with
"No such file". A `FileSystemProvider.write` must create-or-overwrite; fixed to `open_with_flags(CREATE |
TRUNCATE | WRITE)` + `write_all` + `shutdown`.

## Acceptance Criteria
- [x] The in-process test server is filesystem-backed (opendir/readdir/open/read/write/stat/mkdir/remove/
      rmdir over `std::fs`, io-error → `StatusCode` mapping).
- [x] `write` creates a new file (or overwrites), `read` returns it verbatim; `mkdir` → `stat` is a dir;
      `delete` removes a file and a dir — a full round-trip test.
- [x] All prior tests (list/stat/read, host-key, auth, location bridge) still pass against real files.
- [x] `cargo test` (cpe-sftp 9) + clippy `-D warnings` clean on the 3-OS matrix.

## Work Log
- 2026-07-22 — Closes the test gap where write/mkdir/delete were implemented but unexercised. The provider
  is now round-trip-verified end to end; the write create-fix is a genuine correctness improvement.
