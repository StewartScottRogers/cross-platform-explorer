---
id: CPE-1286
title: "Extend file_type magic-byte signature coverage"
type: feature
component: cpe-server
priority: low
status: Done
tags: ready
created: 2026-08-03
epic: CPE-1000
---

## Summary
The pure `file_type` magic-byte detector (powering the true-type + mismatch columns) misses several common
formats. Add signatures + `FileType` variants + canonical extensions. Pure, self-contained, no I/O, no deps.

## Build
- `crates/server/src/file_type.rs` ONLY (self-contained; no command, no bindings). Add detection + a
  `FileType` variant + canonical extension(s) for: SVG (`<?xml`/`<svg` sniff), Matroska/WebM
  (`0x1A45DFA3`), AVI (RIFF + `AVI ` at offset 8 — reuse the existing RIFF-disambiguation pattern used for
  WAV/WebP), SQLite (`SQLite format 3\0`), Java class (`0xCAFEBABE`), ICO (`00 00 01 00`),
  TrueType/OpenType/WOFF/WOFF2 fonts, and xz / zstd / bzip2 archives.
- Extend the existing per-format `#[test]` fixtures (one signature test per new format) and keep the
  container/extension mismatch guards correct (e.g. don't let a ZIP-based `.docx` false-flag).
- If `FileType` is matched exhaustively anywhere (e.g. `column_extract.rs`), update those arms; prefer a
  non-exhaustive match so adding variants stays local to `file_type.rs`.

## Acceptance criteria
- Each new format is detected from its magic bytes with the correct canonical extension; existing formats
  still detected; container formats (docx/zip) not mis-flagged.
- `cargo test -p cpe-server` (new fixtures) green; `cargo clippy` clean both feature modes; no new dep; no
  behavior change to unrelated code.

## Notes
Pure signature work — highest parallelism, zero shared surface with the other Shift-1 tickets. Epic
CPE-1000 (true-type/mismatch columns already shipped; this widens coverage). Feeds CPE-1285's tree sweep.

## Work Log
- 2026-08-03 — file_type +13 formats merged (#583). Reviewer APPROVE, all 13 signatures byte-verified vs specs, 50/50 file_type tests, clippy clean both modes. Only touched file_type.rs (clean merge).
