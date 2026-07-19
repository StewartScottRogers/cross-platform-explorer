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
