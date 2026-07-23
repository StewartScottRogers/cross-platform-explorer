---
id: CPE-877
title: End-to-end zip-slip guard test for archive extraction
type: chore
component: Server
priority: low
tags: ready
epic: CPE-614
created: 2026-07-21
closed: 2026-07-21
status: Done
---

## Summary
`archive::extract_archive` relies on the `zip` crate's `enclosed_name` to neutralise path-traversal
("zip-slip") entries, but that safety was asserted only by a code comment — no test. The 7z path has an
`entry_name_is_safe` unit test; the far more common **zip** format had no end-to-end coverage, so a future
`zip`-crate bump that regressed traversal handling would ship silently.

Added a security regression test that hand-crafts a minimal STORED zip whose entry name is `../escape.txt`
(the zip *writer* rejects such a name, so the bytes are built by hand with a valid CRC-32) and asserts the
entry is **never** written outside the extraction root — whether the crate rejects the archive or skips the
entry. No behavior change; test-only.

## Acceptance Criteria
- [x] A zip containing `../escape.txt` cannot write outside the extraction dir (verified end-to-end).
- [x] Test is robust to both safe outcomes (crate errors on the bad archive, or skips the entry).
- [x] `cargo test` + `cargo clippy --all-targets -D warnings` green in `cpe-server`.

## Work Log
- 2026-07-21 (autonomous) — Audited `archive.rs`: the guards are correct (7z `entry_name_is_safe`, tar
  checked `unpack`, zip `enclosed_name`). Confirmed the zip *writer* itself refuses `../` names, and the
  *reader* rejects an archive containing one — so extraction returns Err rather than escaping. Locked the
  invariant in with a hand-built malicious zip + CRC-32 helper. 9/9 archive tests pass; clippy clean.
