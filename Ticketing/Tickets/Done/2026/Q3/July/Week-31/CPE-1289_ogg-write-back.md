---
id: CPE-1289
title: "OGG Vorbis write-back codec"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-725
---

## Summary
`media_meta` reads OGG/Vorbis comments (`read_ogg`) and has a `write_vorbis_comment` helper, but OGG is not
writable (`is_writable` = mp3/flac/jpeg only) — the epic explicitly deferred OGG write as "repaging
complexity". Add an OGG Vorbis write-back codec so Vorbis-comment edits round-trip. Headless, cargo
round-trip-tested.

## Build
- New `pub fn write_ogg(orig: &[u8], fields: &[MetaField]) -> Result<Vec<u8>, String>` in
  `crates/server/src/media_meta_write.rs`: build the new Vorbis comment header packet via the existing
  `write_vorbis_comment`, then splice it into the Ogg bitstream — replace the comment packet (the second
  Vorbis header packet) and **re-page** the affected region: recompute Ogg page segment-table lacing,
  page sequence numbers, and per-page CRC32 (Ogg's polynomial `0x04c11db7`, no reflection), preserving the
  identification header packet and the audio pages. Handle a comment header that spans pages.
- Wire `"ogg" | "oga"` into `media_meta::write_back` and `is_writable` (`media_meta.rs`).
- No new dependency (reuse the existing Vorbis + Ogg parsing already in `read_ogg`; add CRC + paging by
  hand). Never panic — malformed/non-Ogg input → `Err`.

## Acceptance criteria
- Round-trip: `read_all("ogg", …)` → edit a field via `write_back` → `read_all` shows the edit; the
  identification header + audio data are preserved and the output is a valid Ogg stream (page CRCs verify,
  granule positions/serial preserved); `is_writable("ogg")` true.
- `cargo test -p cpe-server` green (round-trip + a CRC-correctness check + a malformed-input Err case);
  `cargo clippy` clean both feature modes; no new dep; existing mp3/flac/jpeg write tests still pass.

## Notes
Hard but pure + testable (Ogg paging + CRC32). Epic CPE-725. Shares `media_meta_write.rs` with the merged
EXIF write (CPE-1288) — build on current main.

## Work Log
- 2026-08-03 — OGG Vorbis write-back merged (#591, ff-landed after media_meta.rs union resolve). Reviewer (opus) APPROVE: CRC provably correct non-reflected Ogg variant (KAT 0x89A1897F + dual-impl agreement), re-paging (lacing/continuation/BOS/EOS/seq) correct incl 70KB multi-page, output re-parses thru read_ogg with every page CRC re-validated, panic-safe, multiplexed/chained refused. 1417 green clippy clean.
