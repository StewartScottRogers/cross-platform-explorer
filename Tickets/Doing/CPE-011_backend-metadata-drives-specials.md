---
id: CPE-011
title: Backend — richer file metadata, drives, and special folders
type: Feature
status: Open
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

The Windows 11 Explorer UI needs data the backend does not expose: modified date, file type, drive
list, and the special folders (Desktop, Documents, Downloads, Pictures, Music, Videos) used by the
navigation sidebar and Quick access grid.

## Acceptance Criteria

- [ ] `DirEntry` gains `modified` (epoch ms) and `extension`
- [ ] New command `list_drives` returns available drives/roots
- [ ] New command `special_folders` returns Desktop/Documents/Downloads/Pictures/Music/Videos
- [ ] Cross-platform: works on Windows and POSIX, no new crates that risk the build
- [ ] Rust unit tests cover the helpers
- [ ] `cargo check` + `clippy` clean in CI

## Resolution
## Work Log
## Notes

Use `std::time` for timestamps (no chrono) to avoid adding dependencies that cannot be compiled locally.
