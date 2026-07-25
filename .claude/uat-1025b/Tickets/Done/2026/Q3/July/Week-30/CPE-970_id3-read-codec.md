---
id: CPE-970
title: ID3v2 audio-tag read codec (media-metadata read)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-725
estimate: 2-3h
---

## Summary
Next per-format codec for CPE-725 (media metadata studio), also feeding CPE-707 (metadata columns). CPE-942
(`media_meta_edit::apply_edits`) is the edit *policy* but "no file parsing here — the codec layer reads the
fields in"; nothing actually **reads** tags from a file yet. This lands the first read codec: a pure
`cpe-server::media_meta_read::read_id3v2(bytes) -> Vec<MetaField>` that parses an ID3v2 tag (the ubiquitous
MP3 tag) into the existing `media_meta_edit::MetaField` model, so the studio can display/edit real tags and
a column extractor can surface Artist/Album/Title/Track.

Dayshift tier-4 (2026-07-24), continuing the highest-value undone **headless** epic children after the
snapshot store (CPE-969).

## Design (pure, byte-parse → `MetaField`)
- `read_id3v2(bytes) -> Vec<MetaField>` — recognises the `ID3` header (v2.2 / v2.3 / v2.4), reads the
  syncsafe tag size, and walks frames within it. Non-ID3 or truncated input → empty vec, **never a panic**
  (all reads bounds-checked).
- **Text frames** (`T***`, and the v2.2 3-char `TT2`/`TP1`/…): decode the encoding byte
  (Latin-1 / UTF-16+BOM / UTF-16BE / UTF-8), strip trailing NULs, map known frame ids → friendly keys
  (Title, Artist, Album, Album Artist, Track, Disc, Genre, Year, Composer, Publisher, BPM, Copyright);
  unknown `T***` frames pass through under their raw id so nothing useful is lost.
- **`COMM`** comment frames decoded (skip the 3-byte language + the description, keep the text).
- All fields land in group `"id3"`, `editable: true` (these are user-editable tags), ready for
  `apply_edits` and the future write codec.

## Acceptance Criteria
- [x] `media_meta_read::read_id3v2` parses v2.3 + v2.4 (and v2.2 3-char) text frames into `MetaField`s with
      friendly keys (Title/Artist/Album/…), group `"id3"`, `editable: true`; unknown `T***` frames pass
      through under their raw id.
- [x] Encodings handled: Latin-1, UTF-8, UTF-16 (BOM) + UTF-16BE — asserted per encoding
      (`reads_latin1_and_utf8_text_frames_v23`, `reads_utf8_and_utf16_and_v24_syncsafe_sizes`).
- [x] Robust: non-ID3 bytes, every truncation offset, zero-size frame, trailing padding — sane partial
      result, no panic (`non_id3_input_yields_nothing`, `truncated_tag_does_not_panic_and_returns_partial`,
      `stops_cleanly_on_padding_and_zero_sized_frames`). `COMM` comments decoded (lang + desc skipped).
- [x] Cargo-tested (9 tests) with in-test synthesised tags (no binary fixtures); clippy clean both modes;
      no new deps (pure std).

## Work Log
- 2026-07-24 (dayshift) — Built `cpe-server::media_meta_read::read_id3v2`. Handles v2.2/2.3/2.4 headers,
  syncsafe vs plain frame sizes, an extended header (skipped best-effort), the 4 text encodings, and `COMM`.
  All reads bounds-checked; a lying tag size is clamped to the buffer so it can't over-read. `friendly_key`
  returns an owned `String` (no `'static` leak on unknown-frame passthrough). 9 tests, clippy clean both
  modes. Write-back codec + studio UI remain in CPE-725; sibling EXIF/Vorbis/PDF read codecs later.

## Notes
- Read-only codec; the write-back codec + studio UI remain in CPE-725. Vorbis/FLAC/EXIF/PDF read codecs are
  siblings for later. Also feeds CPE-707: a column extractor maps a chosen `MetaField` → `CellValue`.
