---
id: CPE-908
title: tar.gz archive creation + format-dispatching compress
type: feature
component: Backend
priority: medium
tags: ready
epic: CPE-705
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
First headless slice of the archive suite (CPE-705). The archive **read** + **zip create** +
**extract** backends already existed (`archive.rs`: `compress_to_zip`, `extract_archive`, the zip-slip
guard — CPE-251/252/242); the gap was **tar.gz creation** (the standard Unix archive format) and a
single format-dispatching entry point.

Added to `cpe_server::archive`:
- `compress_to_targz(paths, dest)` — packs files/folders into a gzip-compressed tarball, with a recursive
  `tar_add_path` that mirrors the zip walker (directory entries so empty folders survive; never packs the
  output archive into itself, CPE-632).
- `compress_archive(paths, dest)` — dispatches by `dest` extension: `.zip` → zip, `.tar.gz`/`.tgz` →
  tarball, else a clear "unsupported format" error.

## Acceptance Criteria
- [x] `compress_to_targz` round-trips: create → the existing reader lists both files → `extract_archive`
      restores byte-exact contents.
- [x] `compress_archive` dispatches `.zip`/`.tar.gz`/`.tgz` and errors on an unknown extension.
- [x] Empty selection errors; the output archive isn't packed into itself. 11 archive tests, clippy clean, 3-OS.

## Work Log
- 2026-07-22 — Grep-first confirmed zip-create + extract already shipped; filled the tar.gz-create gap +
  the dispatcher. The compress/extract **context actions + navigate-into-archive routing + password
  support** are the remaining (GUI/backend) children of CPE-705.
