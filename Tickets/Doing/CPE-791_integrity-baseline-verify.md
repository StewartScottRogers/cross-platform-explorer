---
id: CPE-791
title: Integrity baseline store + on-demand verify
type: feature
status: Open
priority: low
component: Multiple
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-737
estimate: 3-4h
---

## Summary
Persist a chosen folder's checksum baseline (reusing the sha256 backend) and re-scan on demand, diffing via
CPE-790 to flag unexpected changes / missing files. Opt-in; no background scanning unless configured.

## Acceptance Criteria
- [x] Baseline a folder (recursive sha256 + size + mtime) — backend `checksum_folder`. *(persist + verify-report is the frontend glue, slice 2 with CPE-792.)*
- [ ] Opt-in; nothing scans unless the user baselines/verifies; large trees stay responsive (streamed).
- [ ] check + suite green.

## Notes
Prereq: CPE-790. Reuse the checksum backend; a scheduled verifier is a later follow-up.

## Work Log
2026-07-20 (nightshift restart) — Grep-first confirmed no recursive folder-checksum exists (only single-file
`hash_file`, CPE-412) — safe to build. **Slice 1 (backend) landed:** `checksum_folder(path) -> Vec<ChecksumEntry>`
in `lib.rs` — recursive sha256 + size + epoch-ms mtime per file, symlinks not followed, unreadable files
skipped, sorted by path (stable diff). Shape matches the CPE-790 `ChecksumEntry`. cargo test
`checksum_folder_hashes_files_recursively` (recursion, per-file hash match, size/mtime) passes; clippy
`--all-targets -D warnings` clean. Remaining (slice 2, pairs with CPE-792): frontend persist-baseline +
call `checksum_folder` + diff via CPE-790 `verifyManifest` + report.

