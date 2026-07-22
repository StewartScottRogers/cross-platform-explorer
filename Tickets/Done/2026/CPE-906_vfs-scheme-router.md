---
id: CPE-906
title: VFS scheme router — open a provider from a saved connection
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
The integration seam tying the connections model to the concrete remote providers. New crate
**`crates/vfs` (`cpe-vfs`)**: `open(connection, secret, known_hosts, policy) -> Box<dyn FileSystemProvider>`
— dispatches by scheme to `cpe-sftp` (`sftp`/`ssh`) or `cpe-webdav` (`webdav`/`davs`/`dav`), an unsupported
scheme is a clear error.

- **Secrets stay out of the profile:** the `Connection` (from `cpe_server::connections`) carries only
  metadata; the app fetches the password / key-passphrase from the OS keychain and passes it as `secret`.
- SFTP: builds `SftpAuth::Password` or reads the private key from `key_path` (passphrase = `secret`), then
  connects with host-key verification. WebDAV: maps `host:port + path` to a base URL (`davs`→`https`) with
  optional Basic auth.

## Acceptance Criteria
- [x] `open` dispatches sftp/ssh → cpe-sftp, webdav/davs/dav → cpe-webdav, else a clear "unsupported
      scheme" error.
- [x] Config mapping is pure + tested: WebDAV base URL (scheme/host/port/path), SFTP auth (password vs a
      key read from disk with a passphrase, and a missing-key error).
- [x] Dispatch verified end to end: a sftp connection to a dead port yields an SFTP-flavoured error
      (routed to the SFTP provider), a webdav connection opens lazily.
- [x] 5 tests; clippy `-D warnings` clean; wired into the 3-OS Server-crates CI job.

## Work Log
- 2026-07-22 — The capstone that makes the two remote providers usable via a saved connection + a keychain
  secret. The app now needs only: load connections → fetch secret from keychain → `vfs::open(...)` →
  browse/transfer via the returned provider. The sidebar UI + keychain read/write are the attended
  remainder (CPE-683 / epic CPE-616).
