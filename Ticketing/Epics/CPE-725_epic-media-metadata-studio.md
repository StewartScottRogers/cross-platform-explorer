---
id: CPE-725
title: "EPIC: Media metadata studio (editable EXIF / IPTC / ID3)"
type: Task
status: Proposed
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

2026-07-25 (workshift) — **CPE-1034** added the EXIF read codec `media_meta_read::read_exif` — parses an
image's EXIF (via the existing `kamadak-exif` dep, no new dependency) into group-`"exif"` `MetaField`s
(Make/Model/DateTimeOriginal/exposure/ISO/focal/GPS + descriptive tags), with camera intrinsics read-only
and ImageDescription/Artist/Copyright/UserComment editable. Same bounds-checked never-panic shape as the
audio codecs; also feeds CPE-707 image columns. Independently reviewed (fixture legitimacy empirically
verified) + UAT-passed via an independent EXIF-construction path (PR #351). The read-codec arc now covers
ID3/FLAC/OGG audio + EXIF images. Remaining CPE-725 scope: write-back codecs + the studio editor UI.

2026-07-25 (workshift) — **CPE-1035 DONE (PR #352): first WRITE-BACK codec.** `media_meta_write::write_id3v2`
serialises edited `MetaField`s into a fresh ID3v2.4 tag prepended to the audio payload (strip-and-replace,
idempotent) — the studio can now *edit* audio tags, not just read them. Pairs `read_id3v2` (CPE-970) +
`apply_edits` (CPE-942); independently reviewed (opus, APPROVE) + UAT-passed (full read→edit→write→read
chain, audio byte-preserved). Remaining 725 scope: Vorbis/FLAC + EXIF write-back codecs, then the studio
editor UI (attended). Read arc also expanding this shift: PDF doc-info (CPE-1036) + MP4/MOV video
(CPE-1037) read codecs in review.

2026-07-25 (workshift) — **CPE-1037 DONE (PR #353): video read codec.** `video_meta_read::read_mp4`
extends the read arc to MP4/MOV video (iTunes `moov▸udta▸meta▸ilst` atoms → `MetaField{group:"video"}`).
Independently reviewed (APPROVE + fuzzed) + UAT-passed. Read coverage now spans audio + image + video;
PDF documents (CPE-1036) in review. Remaining 725: FLAC/EXIF write-back + the studio editor UI (attended).

2026-07-25 (workshift) — **CPE-1038 DONE (PR #355): FLAC/Vorbis write-back.** Second write-back codec
after ID3 (CPE-1035) — the studio can now edit-and-save both MP3 (ID3v2) and FLAC (Vorbis) tags.
Independently reviewed + UAT-passed. Write-back arc so far: ID3 ✓, FLAC/Vorbis ✓; remaining: OGG
(deferred, repaging complexity) + EXIF write-back, then the studio editor UI (attended).

2026-07-25 (workshift) — **CPE-1036 DONE (PR #354): PDF document-info read codec.** Read arc now spans
audio (ID3/FLAC/OGG) + image (EXIF) + video (MP4/MOV) + documents (PDF /Info). QA gate caught a real
object-resolution bug pre-merge (fixed + regression-tested). Remaining 725: EXIF/OGG write-back (complex),
studio editor UI (attended).

2026-07-25 (attended) — **CPE-1041 DONE (PR #358): the Metadata Studio ships.** The editable tabbed
inspector is live end-to-end — backend commands (read/writable/atomic-write) + the `MetadataStudioDialog`
(edit+save mp3/flac tags, batch-apply, view-only for pdf/exif/video/ogg). User verified the real installed
build. This is the studio UI the epic was building toward; audio (ID3/FLAC) is now fully round-trip
editable in the GUI. Remaining: OGG/EXIF write-back codecs (deferred, format-risky), plus batch-media
ops + photo-map extras.

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Done-candidate:** all children complete — review DoD; may qualify for Done rather than a re-build.
