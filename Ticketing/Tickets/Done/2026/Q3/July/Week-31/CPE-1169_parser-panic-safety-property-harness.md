---
id: CPE-1169
title: "Parser panic-safety property harness: one table-driven adversarial battery across every byte-parser entrypoint"
type: chore
component: Testing
priority: medium
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-1002
---

## Summary
Workshift-scouted (2026-07-31). `crates/server` has grown a large family of hand-rolled byte parsers
(magic-byte type detection, ID3/FLAC/OGG/EXIF/PDF/MP4 metadata read+write codecs, column extractors, text
encoding, perceptual hash, …). Each module already documents and unit-tests its own "never panics on
malformed input" contract individually. This ticket adds the missing **cross-cutting** proof: one
table-driven property harness that feeds *every* parser entrypoint the same adversarial battery through
`catch_unwind`, so a regression names the exact entrypoint + input class rather than relying on each
module's bespoke fixtures to happen to catch it. This is the guardrail behind `CLAUDE.md`'s "filesystem
commands skip entries they can't read rather than failing the whole listing" — that only holds if no
parser feeding that path can panic.

## Build
- Discover the real set of public byte-parser entrypoints across `crates/server/src/` (type detection,
  media/metadata read+write codecs, column extractors, text-encoding/inspection, perceptual hash).
- Add `crates/server/tests/parser_panic_safety.rs`: a shared adversarial battery (empty, 1-byte,
  truncated at header boundaries, all-zeros, all-`0xFF`, seeded deterministic pseudo-random — a tiny
  inline LCG, no `rand` dependency — valid-magic-then-garbage, overflowing length fields) driven through
  every entrypoint via `std::panic::catch_unwind`, naming the entrypoint + input class on any failure.
- If the harness surfaces a real panic, fix it minimally at the parser (bounds-check before the
  offending slice/index).

## Acceptance Criteria
- [x] One table-driven harness in `crates/server/tests/parser_panic_safety.rs` exercises every discovered
      byte-parser entrypoint against the full adversarial battery via `catch_unwind`.
- [x] `cargo test -p cpe-server` and `cargo clippy --all-targets -D warnings` (both default and `index`
      feature) are green.
- [x] No new dependencies (`rand` or otherwise).

## Work Log
- 2026-07-31 — **Discovered** every public byte-parser entrypoint in `crates/server/src/` (via
  `grep 'pub fn .*\[u8\]'` + reading each module): `file_type::{detect_type, mismatch}`,
  `archive_format::detect_format`, `media_meta_read::{read_id3v2, read_flac, read_ogg,
  parse_vorbis_comment, read_exif, read_pdf}`, `media_meta_write::{write_id3v2, write_flac}`,
  `media_meta::{read_all, write_back}`, `video_meta_read::read_mp4`, `video_column::video_cell`,
  `image_column::image_dimensions_cell`, `doc_column::doc_pages_cell`, `text_encoding::detect_encoding`,
  `perceptual::phash`, `thumb_orient::read_exif_orientation`, `inspect::inspect_bytes`,
  `column_extract::{read_audio_tags, extract_column}` (the last across all `MetaColumn` families: Audio,
  ImageDimensions, DocPages, VideoDuration, and the file-agnostic magic-byte-detector columns).
  `finder_tags::decode` and `archive.rs`'s entry points were reviewed too: `finder_tags::decode` wraps
  the external `plist` crate (already leniently documented/tested) and `archive.rs`'s readers dispatch to
  the external `zip`/`tar`/`sevenz-rust` crates over file **paths**, not raw bytes, so neither is a
  hand-rolled byte-parser in the sense this ticket targets; left out of the harness on that basis.
- **Built** `crates/server/tests/parser_panic_safety.rs`: a shared `battery(magic, header_len)` generates
  ~40 adversarial inputs per entrypoint (empty / 1-byte / all-zeros and all-`0xFF` at 10 sizes / seeded
  LCG pseudo-random at 10 sizes / truncated at every prefix of the magic + around the header boundary /
  valid-magic-then-garbage (zeros, `0xFF`, random) at 3 tail sizes / 4 overflowing-length-field variants
  (`u32`/`u64`, BE/LE, all-1 bits) right after the magic). `assert_no_panic` wraps each
  entrypoint-under-battery call in `catch_unwind`, re-raising any panic (the parser's own, or a graceful-
  contract assertion failing inside the check) as one message naming the entrypoint + input class. 27
  `#[test]` functions cover the full entrypoint list above; each also asserts the specific documented
  graceful sentinel (empty `Vec`/`None`/`CellValue::Empty`/`EncodingGuess::Empty`/etc.) on the unmistakably
  safe `bytes.is_empty()` case — deliberately not over-asserting "must be empty" against every other
  battery class, since a couple of entrypoints have legitimate, already-documented magic-byte collisions
  with parts of the battery (e.g. `file_type::detect_type` reads an all-`0xFF` 2-byte prefix as a valid
  MP3 frame sync per its own documented signature ordering) where that would be a wrong assertion, not a
  real bug. Deterministic pseudo-random via a tiny inline LCG (`lcg_bytes`) — no `rand` dependency added.
- **Result: no real panic surfaced.** Every one of the ~1,000+ generated adversarial cases across all 27
  entrypoints passed on the first run — the module-level bounds-checking this crate already invests in
  (documented in nearly every parser's doc comment: "every offset is bounds-checked... never panics")
  held up under this cross-cutting battery. No parser fix was needed; the payoff here is the standing
  regression harness itself, not a bug find this time.
- **Verify:** `cargo test --test parser_panic_safety` → 27 passed. Full suite: `cargo test` (default
  features) → 1096 unit tests + all integration tests green, including this file's 27. `cargo test
  --features index` → same, green. `cargo clippy --all-targets -D warnings` (default) → clean. `cargo
  clippy --all-targets --features index -- -D warnings` → clean. No new dependencies.

## Notes
- Scoped to `crates/server`'s hand-rolled parsers per the ticket brief; parsers that only wrap an
  external, already-fuzzed crate (`image`, `exif`, `plist`, `zip`/`tar`/`sevenz-rust`) are exercised
  incidentally where our own code calls them (`image_dimensions_cell`, `phash`, `read_exif`,
  `read_exif_orientation`) but weren't the primary target — this crate's own bounds-checking logic was.
