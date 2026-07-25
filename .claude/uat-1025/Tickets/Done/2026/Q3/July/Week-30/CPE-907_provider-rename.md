---
id: CPE-907
title: FileSystemProvider rename/move across all backends
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
`rename`/move is a core file-manager operation the `FileSystemProvider` trait was missing. Added
`fn rename(&mut self, from, to)` to the trait and implemented it on every backend so a file or a whole
directory subtree can be moved within a provider:

- **Local** (`std::fs::rename`), **FakeProvider** (remaps the file key, or the dir marker + every key
  under `from/` to `to/`).
- **SFTP** (`SftpSession::rename`), **WebDAV** (`MOVE` with an absolute `Destination` URL + `Overwrite: T`).

The in-process SFTP + WebDAV test servers gained the matching op (`SSH_FXP_RENAME` → `fs::rename`; `MOVE`
→ `fs::rename`).

## Acceptance Criteria
- [x] `rename` on the trait; implemented for Local / Fake / SFTP / WebDAV.
- [x] File rename + directory-subtree rename verified (FakeProvider), and a file MOVE round-trip over the
      real SFTP and WebDAV transports (old path gone, new path readable).
- [x] `cargo test` (server 188 / sftp 14 / webdav 5 / vfs 5) + clippy `-D warnings` clean, 3-OS.

## Work Log
- 2026-07-22 — Completes the provider operation set (list/stat/read/write/mkdir/delete/**rename**) so every
  backend supports move. The app-side "rename/move" command wiring is the attended remainder (epic CPE-616).
