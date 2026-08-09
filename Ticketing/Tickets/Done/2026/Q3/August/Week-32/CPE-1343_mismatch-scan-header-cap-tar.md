---
id: CPE-1343
title: "type_mismatch_scan: HEADER_CAP (64B) too small to reach TAR's offset-257 magic — disguised .tar never flagged"
type: Bug
status: Done
priority: Medium
component: cpe-server
tags: [ready]
epic: CPE-1000
created: 2026-08-05
closed: 2026-08-05
---

## Problem

`crates/server/src/type_mismatch_scan.rs` reads only `HEADER_CAP = 64` bytes per file
(`type_mismatch_scan.rs:29-33`, `read_capped_header` at ~:84-89) before calling
`file_type::mismatch`. Its doc comment claims 64 bytes "comfortably covers every signature
`detect_type` checks". **That is now false:** CPE-1342 added TAR detection whose `ustar`
magic sits at **byte offset 257** (`file_type.rs`, `matches_at(bytes, 257, b"ustar\0")`),
which needs ≥263 bytes. So the File-Health **Type Mismatch tree-sweep** (CPE-1316, shipped in
`main`) can **never** flag a disguised/renamed `.tar` archive — a real blind spot in a
shipped, security-relevant feature.

It's also **inconsistent** with the per-row column path (`column_cells.rs`, `HEADER_CAP =
1_048_576`), which reads 1 MiB and *does* catch a disguised TAR. Two surfaces of the same
feature disagree.

## Fix

- Raise `HEADER_CAP` in `type_mismatch_scan.rs` to comfortably clear TAR's offset-257 magic —
  e.g. **512** (a full 512-byte tar header block, with margin) is more than enough and still
  trivial to read per file in a tree sweep. (Do not need the column path's 1 MiB here.)
- Fix the now-stale doc comment to state the real reason for the chosen size (must reach the
  deepest offset-based signature `detect_type` checks — TAR at offset 257 — not "no further
  than byte 12").
- Add a regression test mirroring the existing `pe_disguised_as_jpg_is_flagged...` style: a
  file with `ustar\0` at offset 257 (pad the first 257 bytes) claiming a `.txt`/`.jpg`
  extension **is** flagged as a mismatch; a real `.tar` under its own extension is **not**.

## Acceptance criteria

- A real TAR (`ustar` at offset 257) renamed to a foreign extension is flagged by
  `find_type_mismatches` / the scan entrypoint; under `.tar` it is not.
- Existing per-format mismatch-scan tests still pass; no other format regresses.
- Doc comment accurate. `cargo test` + `clippy --all-targets -D warnings` (both feature
  modes) green. No new deps.

## Notes

Pure `cpe-server`, headless, cargo-testable. Single file (`type_mismatch_scan.rs`) — no
overlap with CPE-1344 (which touches `file_type.rs`). Surfaced by the 2026-08-05 frontier scan.
Feeds epic CPE-1000.

## Work Log
- 2026-08-05 (sprint): PR #638 squash-merged to main (25ec5128). Worker(sonnet); Reviewer APPROVE + UAT PASS (independent probe confirmed disguised .tar now flagged, genuine .tar not). All backend/server/sidecar/frontend CI green on 3 OS. HEADER_CAP 64->512; no new deps. Surfaced by the frontier scan (a real inconsistency introduced by CPE-1342 TAR detection vs the 64B scan cap).
