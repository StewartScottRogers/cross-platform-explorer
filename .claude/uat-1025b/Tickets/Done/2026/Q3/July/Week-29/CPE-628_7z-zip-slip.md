---
id: CPE-628
title: "Security: 7z extraction was vulnerable to zip-slip path traversal"
type: Bug
component: Backend
priority: high
status: Done
tags: ready
estimate: 1h
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`extract_archive` unpacked `.7z` files with `sevenz_rust::decompress_file`, which joins each raw entry
name onto the destination **without any path-traversal check** (`de_funcs.rs`: `dest.join(entry.name())`).
A malicious `.7z` with an entry named `..\..\evil` would therefore be written **outside** the chosen
folder — arbitrary file write (a "zip-slip" vulnerability). The zip (crate `enclosed_name`), tar (crate
checked `unpack`), and gz (`file_stem`) paths were already safe; only 7z was exposed.

## Acceptance Criteria
- [x] 7z extraction validates every entry and refuses to write outside `dest`.
- [x] A shared `entry_name_is_safe` guard rejects `..`, absolute paths, and drive prefixes on **both**
      separators / every platform (backslash normalised), unit-tested.
- [x] Other formats keep working; unsafe entries are skipped, not fatal.
- [x] cargo test + clippy (both feature modes) clean.

## Resolution
Added `entry_name_is_safe` + `extract_7z_safe` (using `decompress_file_with_extract_fn` +
`default_entry_extract_fn`, gating each entry on the guard) and switched the `.7z` branch to it.
Updated the `extract_archive` doc to note every format is now traversal-guarded. Unit test covers the
traversal/absolute/drive/empty cases on both separators.

## Work Log
2026-07-18 (dayshift) — Found by auditing all archive-extraction paths for zip-slip; zip/tar/gz were
guarded, sevenz-rust 0.6.1 was not (confirmed in the crate source).
