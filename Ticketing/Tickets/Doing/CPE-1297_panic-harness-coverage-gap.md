---
id: CPE-1297
title: "Close parser panic-safety coverage gap (iptc / exif-write / ogg-write / vorbis-write)"
type: test
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
The `parser_panic_safety.rs` adversarial-fuzz harness (CPE-1169) fuzzes most byte-parsers but omits four
reachable ones added recently: `read_iptc`, `write_exif`, `write_ogg`, `write_vorbis_comment`. Wire them in
so a regression names the entrypoint. Pure coverage close, fully disjoint from all other work.

## Build
- In `crates/server/tests/parser_panic_safety.rs` ONLY: add a `#[test]` battery per entrypoint through the
  existing `run_battery`/`assert_no_panic` harness (match the style of the existing `read_*_never_panics` /
  `write_*_never_panics` entries):
  - `read_iptc_never_panics` — JPEG SOI magic (`FF D8 FF E1`), plus empty/short.
  - `write_exif_never_panics` — JPEG SOI magic; feed a small edit-field set; assert `Err`/no-panic on
    malformed input (write fns take `(orig, fields)` — pass a fixed small `&[MetaField]`).
  - `write_ogg_never_panics` — `OggS` magic; fixed small fields.
  - `write_vorbis_comment_never_panics` — empty magic (it takes only `&[MetaField]`, builds a packet) — fuzz
    the field values (long/empty/non-UTF8-ish strings) rather than raw bytes.
- Assert the documented empty-input sentinel where each returns one; never panic on any fuzzed input across
  the harness's size/truncation/garbage classes.

## Acceptance criteria
- Four new `#[test]`s added to `parser_panic_safety.rs`; the full harness passes; each genuinely exercises
  its entrypoint (not a hollow no-op).
- `cargo test -p cpe-server` green; `cargo clippy` clean both feature modes; no new dep.

## Notes
Only touches `crates/server/tests/parser_panic_safety.rs` — fully independent, good parallel warm-up. Epic
CPE-1002 (safety/robustness). Convention: every new byte-parser gets a harness entry.

## Work Log
