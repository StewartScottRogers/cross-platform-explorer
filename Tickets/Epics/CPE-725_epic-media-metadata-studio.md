---
id: CPE-725
title: "EPIC: Media metadata studio (editable EXIF / IPTC / ID3)"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
A dedicated, *editable* metadata inspector spanning formats the read-only Properties EXIF panel doesn't:
IPTC/XMP for photos, ID3/Vorbis/FLAC for audio, codec/container/bitrate/duration for video, and page/author
info for PDFs — with edit-and-write-back, batch-apply across a selection, and photos-on-a-map for GPS tags.

## Why
Properties shows EXIF one-way today. Photographers and archivists need to edit tags, shift timestamps in
bulk, geotag, and strip sensitive metadata for privacy — content-level editing beyond filesystem rename.

## Rough scope (areas, not child tickets)
- Per-format metadata read/write in Rust (EXIF/IPTC/XMP, ID3/Vorbis/FLAC, video containers, PDF).
- A tabbed inspector UI with editable fields and safe atomic writes + undo.
- Batch operations: find/replace, shift-all-timestamps, copy-from-first, strip-metadata.
- Map view for GPS-tagged photos.

## Open questions (resolve at activation)
- Metadata read/write library choices and format coverage.
- Split of responsibilities with the display-only metadata columns ([[CPE-707]]).
- Map rendering with the strict CSP / offline constraints.

## Definition of Done
- Users can edit and write back EXIF/IPTC/XMP/ID3/video/PDF metadata safely, with undo.
- Batch metadata operations (shift timestamps, strip, find/replace) work across a selection.
- GPS-tagged photos can be plotted; Properties' read-only EXIF continues to work.

## Work Log
2026-07-23 (dayshift) — **Activated.** First slice: **CPE-942** — `media_meta_edit::apply_edits`: the pure
set/clear edit policy over EXIF/IPTC/ID3 fields (refusing read-only ones, reporting applied/rejected).
Remaining: the per-format read/write codecs and the studio editor UI.

2026-07-24 (dayshift) — **CPE-970** landed the first read codec: `media_meta_read::read_id3v2` — parses
ID3v2.2/2.3/2.4 audio tags (all 4 text encodings + COMM) into `MetaField`s, robust to malformed input, pure
std. Also feeds CPE-707 columns. Remaining: the **write-back** codec, sibling read codecs (EXIF/Vorbis/FLAC/
video/PDF), and the studio editor UI.

2026-07-24 (dayshift) — **CPE-972** added the second read codec: `media_meta_read::read_flac` + `parse_vorbis_comment` — FLAC/Vorbis tags into `MetaField`s under the **same** friendly keys as ID3, so `media_column::audio_cell` handles FLAC unchanged. Remaining: OGG framing (reuses `parse_vorbis_comment`), write-back codecs, studio UI.

2026-07-24 (dayshift) — **CPE-973** completed the Vorbis codec for OGG: `media_meta_read::read_ogg` reuses `parse_vorbis_comment` via the `vorbis` comment-header signature. The audio read arc (ID3/FLAC/OGG → typed audio columns) now covers the common formats. Remaining: multi-page Ogg reassembly, EXIF/PDF/video read codecs, write-back, and the studio UI.
