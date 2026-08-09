---
id: CPE-1514
title: "cpe-ftp provider (FTP/FTPS) — implement FileSystemProvider via suppaftp, register ftp/ftps in vfs::open"
type: Feature
status: Doing
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1502
created: 2026-08-09
---
## What (first net-new Network protocol — mirrors the shipped cpe-sftp/cpe-webdav pattern)
New `crates/ftp` (`cpe-ftp`) crate implementing the existing `FileSystemProvider` trait (list/stat/read/write/
mkdir/delete/rename) over FTP/FTPS, and a `ftp`/`ftps` scheme arm in `cpe_vfs::open` (`crates/vfs`) + the
`location.rs` URI parser. Once done, an `ftp://host/path` connection browses through the same CPE-1511 command
routing that SFTP/WebDAV already use.

## How
- Crate: **`suppaftp`** (maintained fork of the abandoned/vulnerable `ftp` crate), with the `rustls-ring` TLS
  feature to match the `ring` backend `cpe-sftp` already chose (consistency + no new TLS stack). Sync API — no
  async runtime needed (like `cpe-webdav`). Study `crates/webdav/src/lib.rs` + `crates/sftp/src/lib.rs` for the
  exact provider shape (connect(config, secret, ...) → provider; the FileSystemProvider impl; bounded reads;
  streaming where applicable) and mirror it.
- Auth: **user+pass** and **Anonymous** (anonymous = user `"anonymous"`, password an email-ish placeholder or
  empty) — anonymous is common for public FTP and can be handled directly here without waiting on CPE-1501's
  broader auth-model epic; document the choice. FTPS (explicit TLS) via suppaftp's TLS feature; port 21 default.
- Register `ftp`/`ftps` in `location.rs` (`Scheme`) + `cpe_vfs::open` scheme match (currently returns
  "unsupported scheme" for anything but sftp/webdav). Path traversal: remote names flow through the same
  `is_safe_name`/guarded_join guards CPE-1511 applies at the listing layer — but ALSO ensure the provider's own
  `download_tree`/entry handling can't be fooled (mirror cpe-sftp/cpe-webdav's hardening).
- Bounded reads / resource-exhaustion conventions (never buffer a whole remote file unbounded; stream).

## Verify (HEADLESS)
- The crate's own tests: mirror how `crates/sftp`/`crates/webdav` test their providers. If suppaftp offers an
  in-process/test FTP server or a mock, use it; else use a lightweight local FTP server fixture spun up in the
  test (gated/`#[ignore]` if it needs a real server binary). At minimum: connect (mock), list a dir, stat a file,
  read a file, error on bad auth — no panic, bounded.
- **vfs routing:** a test proving `cpe_vfs::open` dispatches an `ftp://` URI to `cpe-ftp` (and `ftps://` too).
- `cargo test` (crates/ftp + crates/vfs + crates/server as touched) green; `cargo clippy --all-targets -D
  warnings` both feature modes; **commit any Cargo.lock delta incl `src-tauri/Cargo.lock`** if the app pulls the
  new crate (suppaftp is a new dep — Dependency Steward: justified, the maintained FTP crate, rustls-ring feature
  no new TLS stack; regenerate + commit all affected lockfiles). Regenerate `bindings.gen.ts` only if a
  command/specta surface changed (likely not — this is provider-internal).

## Ship
Move CPE-1514 Doing→Done, Work Log (crate shape, auth handling, scheme registration, dep+lockfile note, tests).
The Network sidebar (CPE-1513) will let a user add an `ftp://` connection with no further UI work. Effort S–M.
