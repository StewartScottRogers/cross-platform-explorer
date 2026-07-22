---
id: CPE-899
title: SFTP FileSystemProvider over russh + in-process test harness (unblocks CPE-682)
type: feature
component: Backend
priority: high
tags: ready
epic: CPE-616
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
Unblocks CPE-682 (the SFTP provider's network half). New standalone crate **`crates/sftp` (`cpe-sftp`)**:
an SFTP `FileSystemProvider` (epic CPE-616) over **`russh` + `russh-sftp`** (pure Rust — no libssh2/C),
with **host-key verification delegated to `cpe_server::known_hosts`** at connect time.

Key decisions (made per the green-light; see the research the user requested):
- **`russh` + `russh-sftp`, in-process test server.** Chosen over Docker/`atmoz/sftp` because our backend
  CI is a 3-OS matrix and **Docker only runs on the Linux runner** — an in-process pure-Rust server runs
  identically on Linux/macOS/Windows. (Rejected: the stale `sftp-server` crate; client-only `sftp-rs`.)
- **`ring` crypto backend, not the default `aws-lc-rs`** — aws-lc-sys needs NASM/cmake to build (fails on
  the Windows CI leg); `ring` is self-contained.
- **Async isolated in this crate.** The provider owns a small internal tokio runtime and presents the
  **sync** `FileSystemProvider`, so the lean std-only `cpe-server` core is untouched.
- **Host-key policy** `Strict` (only already-`Trusted`) vs `Tofu` (Trusted **or** Unknown-first-use);
  `Changed`/`Revoked` are always refused — the `check_server_key` hook maps a presented key → the
  `known_hosts` verdict.

## Status / what's proven
- Provider: `connect` (password auth) + `list`/`stat`/`read`/`write`/`mkdir`/`delete` over `SftpSession`.
- **Full happy path proven end-to-end over a real in-process SSH/SFTP handshake:** host-key verification
  (Trusted) → `list` → `stat` → `read`, plus a **TOFU accept** of an unknown host that surfaces its key.
- **Host-key security proven:** a **changed** key is refused (MITM), an **unknown** key is refused under
  `Strict`. All three tests pass on the 3-OS CI matrix.

## Acceptance Criteria
- [x] `crates/sftp` builds on the 3-OS matrix with the `ring` backend (no NASM/Docker); wired into CI.
- [x] `SftpProvider` implements `FileSystemProvider`; `connect` verifies the host key via `known_hosts`.
- [x] Changed / Unknown(Strict) host keys are refused end-to-end.
- [x] Happy-path list/stat/read over the in-process server passes (all 3 OSes).

## Work Log
- 2026-07-22 — Built the crate + provider + canned in-process russh-sftp fixture. Confirmed cargo can fetch
  russh here; switched aws-lc-rs→ring for Windows. Initial fixture hang / Linux failure both traced to one
  cause: `tokio::TcpListener::from_std` needs the socket set **non-blocking first** (Windows tolerated a
  blocking socket, which also stalled the async I/O pump so the SFTP VERSION reply never reached the
  client; Linux/macOS panicked outright). Adding `set_nonblocking(true)` fixed both — full connect →
  list/stat/read now green everywhere.
