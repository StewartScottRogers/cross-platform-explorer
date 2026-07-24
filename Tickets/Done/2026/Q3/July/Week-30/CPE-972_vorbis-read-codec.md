---
id: CPE-972
title: FLAC / Vorbis-comment read codec (media-metadata read)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-725
estimate: 2h
---

## Summary
Second audio read codec for the metadata studio (epic CPE-725), sibling to CPE-970 (ID3). Adds
`read_flac(bytes)` + a reusable `parse_vorbis_comment(block)` to `cpe-server::media_meta_read`, decoding the
Vorbis-comment tags FLAC (and OGG) use into the same `MetaField` model — and mapping their keys to the **same
friendly keys** as ID3, so `media_column::audio_cell` (CPE-971) surfaces FLAC Track/Year/Artist columns with
zero extra wiring.

Dayshift tier-4 (2026-07-24), continuing CPE-725's read codecs after CPE-970.

## Design (pure byte-parse → `MetaField`)
- `parse_vorbis_comment(block) -> Vec<MetaField>` — the raw Vorbis-comment structure (LE: vendor-length +
  vendor, comment-count, then each `len`+`KEY=VALUE` UTF-8). Reusable for both FLAC and OGG.
- `read_flac(bytes) -> Vec<MetaField>` — checks the `fLaC` magic, walks FLAC metadata blocks (1-byte
  last/type header + 3-byte big-endian length), and parses the type-4 (VORBIS_COMMENT) block.
- Key mapping (case-insensitive): `TITLE/ARTIST/ALBUM/ALBUMARTIST/TRACKNUMBER/DISCNUMBER/GENRE/DATE/YEAR/
  COMPOSER/ORGANIZATION|PUBLISHER/DESCRIPTION|COMMENT` → the ID3 friendly keys (Title/Artist/…/Year/…);
  unknown keys pass through under their own name. Group `"vorbis"`, `editable: true`.
- Robust: non-FLAC or truncated input → empty/partial, never a panic (all reads bounds-checked).

## Acceptance Criteria
- [x] `parse_vorbis_comment` decodes vendor + `KEY=VALUE` list into `MetaField`s with friendly keys, case-
      insensitively, skipping blank values + malformed (no `=`) entries; unknown keys pass through capitalised.
- [x] `read_flac` walks FLAC metadata blocks, returns the type-4 Vorbis-comment tags; non-FLAC → empty.
- [x] Friendly keys line up with CPE-970 so `media_column::audio_cell` yields typed `Int` Track/Year from
      FLAC tags — asserted end-to-end (`flac_friendly_keys_line_up_with_id3_for_audio_cell`).
- [x] Robust to truncation (every offset) / bad magic / short blocks — no panic. 14 tests total (5 new);
      clippy `--all-targets -D warnings` clean both modes; no new deps (pure std).

## Work Log
- 2026-07-24 (dayshift) — Added `read_flac` + `parse_vorbis_comment` (+ `vorbis_key`, `le_u32`) to
  `media_meta_read`. Reuses ID3's friendly-key vocabulary so FLAC tags flow through `audio_cell` unchanged.
  All reads bounds-checked; a truncated block stops the walk. OGG-page framing (reusing
  `parse_vorbis_comment`) + write-back remain for CPE-725.

## Notes
- OGG-Vorbis container framing (Ogg pages) reuses `parse_vorbis_comment` and is a small sibling for later.
  Write-back codecs + studio UI remain in CPE-725.
