---
id: CPE-252
title: Extract an archive to a folder
type: Feature
status: Done
priority: Medium
component: Backend + Frontend
estimate: 2h
created: 2026-07-13
---

## Summary

Archive browsing (CPE-242) can open an archive and extract a *single* entry to a
temp file, but there is no way to extract a whole archive to disk. Add a
right-click **Extract** that unpacks the selected archive into a new subfolder
named after it, in the current directory.

Supports the formats the browser already reads: zip-family (zip/jar/apk/…),
tar, tar.gz/tgz, gz (single file), and 7z. (ISO stays browse-only for now.)

## Acceptance Criteria

- [ ] New backend command `extract_archive(path, dest)` that creates `dest` and
      unpacks the archive into it, dispatched by extension like
      `read_archive_entries`.
- [ ] Zip extraction uses the crate's path-safe extractor (no zip-slip escape).
- [ ] Right-click a single archive file offers **Extract to <name>/**.
- [ ] The destination subfolder is named after the archive stem, auto-numbered to
      avoid collisions; the listing refreshes and the new folder is selected.
- [ ] Action is unavailable in Home and inside the read-only archive view.
- [ ] Round-trip verified: compress (CPE-251) then extract restores the files.
- [ ] `cargo test` (backend) and `npm run check` pass.

## Resolution

Added backend `extract_archive(path, dest)` that creates `dest` and unpacks by
extension like `read_archive_entries`: zip-family via the crate's path-safe
`ZipArchive::extract` (guards against zip-slip), tar and tar.gz/tgz via
`tar::Archive::unpack`, bare `.gz` to its single inner file, and 7z via
`sevenz_rust::decompress_file`. Wired an **Extract** context-menu action shown
only for a single extractable archive; unpacks into a new subfolder named after
the archive (compound suffixes like `.tar.gz` stripped), auto-numbered, then
selects it. Guarded out of Home and the read-only archive view.

## Work Log
2026-07-13 — Filed and picked up during Nightshift. Companion to [[CPE-251]] (compress).
2026-07-13 — Implemented backend + frontend. Verified: cargo test (tar.gz extract
+ zip round-trip), `npm run check`, full vitest (241) + cargo test (59) green,
clippy clean. Landed on branch cpe-251-252-archive-ops.
