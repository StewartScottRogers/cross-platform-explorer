---
id: CPE-973
title: OGG-Vorbis comment read (completes the Vorbis codec)
type: feature
component: Backend
priority: low
tags: ready
status: Done
created: 2026-07-24
epic: CPE-725
estimate: 1h
---

## Summary
Completes the Vorbis-comment read codec (CPE-972) for the other common container: OGG. Adds
`cpe-server::media_meta_read::read_ogg(bytes)` reusing the already-tested `parse_vorbis_comment`, so OGG audio
tags flow into the same `MetaField` model — and, via the shared friendly keys, through
`media_column::audio_cell` — exactly like FLAC and MP3.

Dayshift tier-4 (2026-07-24), a small finisher on the CPE-725 read-codec arc (CPE-970 ID3 → CPE-971 columns
→ CPE-972 FLAC → this).

## Design
- `read_ogg(bytes)` — verifies the `OggS` magic, locates the Vorbis **comment-header** packet by its 7-byte
  signature `\x03vorbis`, and parses the bytes that follow with `parse_vorbis_comment` (which reads exactly
  its declared entries and ignores trailing framing). Non-OGG or no-signature → empty; truncation-safe.
- **Pragmatic scope (documented):** assumes the comment header isn't split across Ogg pages (true for
  typical tag sizes). Full multi-page packet reassembly is a later refinement; `parse_vorbis_comment` stays
  the shared codec.

## Acceptance Criteria
- [x] `read_ogg` extracts Title/Artist/Track from an OGG stream's comment header (`read_ogg_extracts_comment_header`).
- [x] Non-OGG magic / missing signature / every-offset truncation → empty, no panic
      (`read_ogg_rejects_non_ogg_and_tolerates_truncation`).
- [x] Reuses `parse_vorbis_comment` → same friendly keys → `audio_cell` works on OGG unchanged.
- [x] 16 tests total (2 new); clippy `--all-targets -D warnings` clean both modes; no new deps (pure std).

## Notes
- Multi-page Ogg packet reassembly + write-back remain for CPE-725; the read arc (ID3/FLAC/OGG + typed audio
  columns) is now covered for the common audio formats.

## Work Log
- 2026-07-24 (dayshift) — Added `read_ogg` + `find_subslice`; 2 tests. Closes the audio read-codec arc.
