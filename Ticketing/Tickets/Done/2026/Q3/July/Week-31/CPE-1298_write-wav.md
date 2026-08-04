---
id: CPE-1298
title: "write_wav: RIFF LIST/INFO metadata writer (read/write symmetry)"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-725
---

## Summary
`read_wav` reads WAV `LIST`/`INFO` metadata but WAV has no writer (`write_back` returns unsupported;
`is_writable` excludes wav). Add `write_wav` so WAV tag edits round-trip — symmetric with the existing
reader. Headless, cargo round-trip-tested.

## Build
- New `pub fn write_wav(orig: &[u8], fields: &[MetaField]) -> Result<Vec<u8>, String>` in
  `crates/server/src/media_meta_write.rs`: parse the RIFF chunk tree, rewrite (or insert, if absent) the
  `LIST`/`INFO` chunk carrying `INAM`/`IART`/`IPRD`/`ICRD`/`ICMT`/`IGNR` from the edited fields (same friendly
  keys `read_wav` uses), preserving all other chunks (`fmt `, `data`, etc.) byte-for-byte, honoring even-byte
  chunk padding, and UPDATING the outer `RIFF` chunk size to match the new total. Never panic — malformed/
  non-RIFF input → `Err`.
- Wire `"wav" =>` into `media_meta::write_back` and add `"wav"` to `is_writable` (`media_meta.rs`). (`"wav"`
  is already in `AUDIO_EXTS` from CPE-1291.)
- No new dep.

## Acceptance criteria
- Round-trip: `read_wav(write_wav(orig, edits))` shows the edited INAM/IART/etc.; the `fmt `/`data` chunks +
  audio samples are preserved byte-for-byte; the outer RIFF size is correct; `is_writable("wav")` true;
  malformed input → `Err`, no panic; a WAV with no existing INFO chunk gets one inserted.
- `cargo test -p cpe-server` green; `cargo clippy` clean both feature modes; no new dep; existing
  mp3/flac/jpeg/ogg write tests still pass.

## Notes
Epic CPE-725 (media studio). Shares `media_meta_write.rs` + `media_meta.rs` with CPE-1288/1289 (merged) and a
later `write_pdf` (CPE-1300) — sequence write_pdf after this. Do NOT add write_wav to
`parser_panic_safety.rs` here (keeps it disjoint from CPE-1297); a one-line harness follow-up can come later.

## Work Log
- 2026-08-03 — write_wav merged (#596). Reviewer APPROVE: RIFF size recomputed (no drift), even-padding correct, id-mapping exact inverse of read_wav, idempotent, fmt/data byte-preserved (tested), 118/118 re-run clippy clean. Non-blocking nit (follow-up): the INFO-form-type peek (media_meta_write.rs ~965) reads from orig directly rather than the chunk-bounded data slice like read_wav does — harmless (.get()-guarded, only on malformed <4B LIST), could tighten to `orig.get(data_start..data_end)?.get(0..4)`.
