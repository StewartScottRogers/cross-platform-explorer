---
id: CPE-011
title: Backend — richer file metadata, drives, and special folders
type: Feature
status: Done
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The Windows 11 Explorer UI needs data the backend does not expose: modified date, file type, drive
list, and the special folders (Desktop, Documents, Downloads, Pictures, Music, Videos) used by the
navigation sidebar and Quick access grid.

## Acceptance Criteria

- [x] `DirEntry` gains `modified` (epoch ms) and `extension`
- [x] New command `list_drives` returns available drives/roots
- [x] New command `special_folders` returns Desktop/Documents/Downloads/Pictures/Music/Videos
- [x] Cross-platform: works on Windows and POSIX, no new crates that risk the build
- [x] Rust unit tests cover the helpers
- [x] `cargo check` + `clippy` clean in CI

## Resolution

Extended `DirEntry` with `modified` (epoch ms, `Option<u64>`) and `extension` (lowercased, dotless).
Added two commands: `list_drives` (enumerates A–Z drive roots on Windows; returns `/` on POSIX) and
`special_folders` (Desktop/Documents/Downloads/Pictures/Music/Videos).

Deliberate choices:
- **No new crates.** Timestamps use `std::time` (`duration_since(UNIX_EPOCH).as_millis()`) rather than
  `chrono`, and drives are probed with `Path::exists()` rather than a winapi crate. Since I cannot
  compile Rust locally, every added dependency is an unverifiable risk to the release build.
- `special_folders` only returns folders that **actually exist**, so the sidebar can never show a link
  that leads nowhere.
- `modified` is `Option` because not every filesystem reports it; the UI renders "" rather than a fake date.

Added 6 more Rust unit tests (extension lowercasing, `.tar.gz` -> `gz`, epoch conversion, drives
non-empty, special folders all exist). Verified by CI: cargo check + clippy + test all green.

## Work Log

2026-07-11 — Picked up after CPE-010 made Rust verifiable in CI.
2026-07-11 — Added modified/extension to DirEntry; added list_drives and special_folders; registered both in generate_handler!.
2026-07-11 — Chose std::time over chrono deliberately: unverifiable deps are a release-build risk when Rust can't be compiled locally.
2026-07-11 — Added 6 Rust unit tests. CI: cargo check, clippy -D warnings, cargo test all green. Closing as Done.

## Notes

Use `std::time` for timestamps (no chrono) to avoid adding dependencies that cannot be compiled locally.
