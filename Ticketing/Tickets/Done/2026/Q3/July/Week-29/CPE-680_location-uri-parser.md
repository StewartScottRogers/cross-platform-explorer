---
id: CPE-680
title: Location model + URI parser (local vs remote)
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-616
estimate: 1-2h
---

## Summary
Safe/headless foundation for CPE-616. A pure Rust module that parses a location string into a structured
`Location { scheme, user, host, port, path }` and classifies it as **local** (a plain path, incl. Windows
drive `C:\...` and UNC `\host\share`) or a **remote scheme** (`sftp`/`ssh`/`smb`/`webdav`/`davs`/`s3`).
No network, no auth, no GUI — just the enabling model so later code can route by scheme rather than
assuming local paths.

## Acceptance Criteria
- [x] `location::parse(&str) -> Location`; a no-scheme string (incl. `C:\`, `/home/x`, `\host\share`)
      is `Scheme::Local` with the whole string as `path`.
- [x] `sftp://[user@]host[:port]/path` (and ssh/smb/webdav/davs/s3) parse into the right parts; missing
      path → `/`.
- [x] `is_local()` helper; `Scheme` + `Location` derive Serialize for future frontend use.
- [x] Thorough unit tests (local drive/UNC/posix, each remote scheme, user@host:port, edge cases);
      clippy clean both feature modes.

## Work Log
2026-07-18 (nightshift) — Picked up as the safe CPE-616 foundation. No questions; best-guess. Est 1-2h.

## Resolution
New pure module `src-tauri/src/location.rs`: `Scheme` (Local/Sftp/Smb/Webdav/S3, Serialize) + `Location
{scheme,user,host,port,path}` + `parse(&str)`. A string without a recognised `scheme://` (POSIX paths,
Windows `C:\`, UNC `\host\share`, or an unknown scheme) is `Local` with the whole input as `path`; a
recognised remote scheme (sftp/ssh→Sftp, smb, webdav/davs/dav, s3) parses `[user@]host[:port]/path`, a
missing path defaulting to `/` and a bad port dropped (not fatal). `#![allow(dead_code)]` keeps it
always-compiled + tested until providers (CPE-681+) consume it. 6 unit tests (local drive/UNC/posix, each
remote scheme, user@host:port, ssh alias, unknown scheme, bad port); 123→ backend tests pass; clippy clean
both feature modes. No network/GUI. First (safe/headless) child of CPE-616. Files: src-tauri/src/location.rs,
src-tauri/src/lib.rs (mod).
