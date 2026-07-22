---
id: CPE-904
title: WebDAV FileSystemProvider — a second remote backend over HTTP/WebDAV
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
A second remote backend for epic CPE-616 (after SFTP): new crate **`crates/webdav` (`cpe-webdav`)** — a
`FileSystemProvider` over HTTP/WebDAV (Nextcloud / ownCloud / many NAS).

- **Synchronous** (unlike SFTP): `ureq` is a blocking HTTP client, so **no internal async runtime** is
  needed — cleaner than the SFTP provider. TLS is pure-Rust (`rustls` + **`ring`**, not aws-lc-rs), so it
  builds with no C tooling / NASM on every CI OS.
- The 6 provider ops map to WebDAV methods: **PROPFIND** (list/stat, `roxmltree`-parsed multistatus),
  **GET** (read), **PUT** (write), **MKCOL** (mkdir), **DELETE** (delete). HTTP Basic auth.
- PROPFIND parsing matches element **local** names, so DAV namespace prefixes (`d:`/`D:`/none) don't
  matter; hrefs are percent-decoded and the collection-self entry is skipped in a Depth:1 listing.

## Decisions
- **`ureq` (sync) over reqwest (async)** — keeps the provider runtime-free, matching the std-only house
  style; `rustls`/`ring` avoids the aws-lc-rs NASM problem that bit the SFTP build.
- **In-process `tiny_http` test server** (fs-backed) — no Docker, so it runs identically on the 3-OS
  matrix (Docker only runs on the Linux runner).

## Acceptance Criteria
- [x] `WebdavProvider` implements `FileSystemProvider` (list/stat/read/write/mkdir/delete) over WebDAV.
- [x] PROPFIND multistatus parsed to entries (name/is_dir/size); Basic auth; clear HTTP-status errors.
- [x] Verified against an in-process fs-backed WebDAV server: list/stat/read, a write/mkdir/delete
      round-trip, and a missing-path error. 3 tests; clippy `-D warnings` clean; wired into CI (3-OS).

## Work Log
- 2026-07-22 — Followed the SFTP provider pattern (provider + in-process test server), but sync — landed
  clean first try (no async pump/handshake debugging). Write-heavy WebDAV niceties (locking, quota,
  chunked PUT) and the app-side scheme routing are follow-ups on epic CPE-616.
