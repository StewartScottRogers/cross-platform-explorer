---
id: CPE-707
title: "EPIC: Custom & metadata columns"
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
Extend the details view with rich, sortable, per-folder columns pulled from file internals: image
dimensions/EXIF, audio ID3 (artist/album/bitrate), video length, PDF page count, and OS extended
attributes/permissions.

## Why
Directory Opus / Finder power users live in metadata columns. The app already has a column framework and
sort infra (CPE-017, columns.ts) to build on; this is the natural next layer.

## Rough scope (areas, not child tickets)
- Rust metadata extractors per family (image/audio/video/document), lazy + streamed per visible row.
- A column-picker UI (add/remove/reorder, per-folder persistence) reusing the columns model.
- Sort/format integration so new columns sort and render like built-ins.
- Coordinate with virtualization (CPE-690) so only visible rows extract metadata.

## Open questions (resolve at activation)
- Extraction cost vs. the 10× perf epic — must only extract for on-screen rows and cache results.
- Per-folder vs. global column sets; how columns persist across sessions.
- Overlap with the media-metadata studio ([[CPE-725]]) — display here, editing there.

## Definition of Done
- Users can add metadata columns from a picker; they sort and format correctly.
- Metadata is extracted lazily for visible rows only, with no regression to open/scroll speed.
- Column choices persist per folder (or per configured scope).
