---
id: CPE-822
title: Extract archive create/extract into cpe-server
type: refactor
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-20
closed: 2026-07-21
epic: CPE-810
estimate: 2-3h
---

## Summary
Follow-up to CPE-815. Move the archive **create/extract** commands (compress to zip CPE-251; extract an
archive CPE-252; extract a single entry CPE-242) — `compress_to_zip`, `extract_archive` (zip/tar/tar.gz/7z),
`extract_archive_entry`, `extract_7z_safe` — into `cpe_server::archive` alongside the already-extracted
archive *listing*. The zip/tar/flate2/sevenz-rust crates already live in `cpe-server`, so this pulls no new
deps and lets `src-tauri` drop them if they become app-unused. Preserve the zip-slip / path-escape safety
(CPE-628/632/419) — those guards move with the code and their tests come along.

## Acceptance Criteria
- [x] `compress_to_zip` / `extract_archive` / `extract_archive_entry` / `extract_7z_safe` move to
      `cpe_server::archive`; the commands dispatch.
- [x] The zip-slip / self-inclusion / escape guards + their tests move with the code and pass headless.
- [x] zip/tar/flate2/sevenz-rust became app-unused and were dropped from `src-tauri` — **and so did `sha2`**
      (its last users, checksum/backup/thumbnail, had already moved). 5 crates slimmed.
- [x] clippy clean both modes; behaviour-preserving.

## Work Log
2026-07-21 — Picked up (follow-up to CPE-815). Moved the archive create/extract commands into the existing
`cpe_server::archive` module: `compress_to_zip` (+ recursive `zip_add_path` with the CPE-632 self-inclusion
skip), `extract_archive` (zip/tar/tar.gz/gz/7z dispatch), `extract_archive_entry` (single entry → temp),
`extract_7z_safe` + the shared `entry_name_is_safe` zip-slip guard (CPE-628), and `tar_unpack`. The three
commands are now thin dispatchers. Ported the unique tests (self-inclusion, tar.gz extract, tar + gzip
listing, entry-name-safe, round-trip) into the crate; removed the app copies.
2026-07-21 — Slimmed: zip/tar/flate2/sevenz-rust + sha2 all became app-unused (production + tests) →
dropped from `src-tauri/Cargo.toml`. Verified: `cpe-server` **105 tests** green; app `cargo test` **66
passed / 0 failed**; clippy `-D warnings` clean on **both** modes.

## Resolution
Extracted the archive **create/extract** side into `cpe_server::archive` alongside the already-extracted
listing (CPE-815 slice 16): `compress_to_zip`, `extract_archive`, `extract_archive_entry`,
`extract_7z_safe`, `entry_name_is_safe` (the shared zip-slip guard, CPE-628), `zip_add_path` (with the
CPE-632 self-inclusion skip), `tar_unpack`. The `compress_to_zip` / `extract_archive` /
`extract_archive_entry` commands are one-line dispatchers. All create/extract + remaining listing tests
moved into the crate. **This retired every app use of `zip`/`tar`/`flate2`/`sevenz-rust` — and `sha2` —
so all five were dropped from `src-tauri`.** Files: `crates/server/src/archive.rs` (extended);
`src-tauri/src/lib.rs` (dispatchers); `src-tauri/Cargo.toml` (5 deps removed).
