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

## Work Log
2026-07-22 (nightshift) — **Activated.** First slice: **CPE-918** — `metadata_column::CellValue` + `compare`
/ `sort_rows` / `display`: the typed cell every metadata column produces, with uniform type-aware sort
(numeric, not lexical; Dimensions by area; Empty pinned last both directions) and human formatting. This is
the "sort/format like built-ins" seam. Remaining: per-family Rust extractors (image/audio/video/doc, lazy
for visible rows only), the column-picker UI (add/remove/reorder), and per-folder persistence.

2026-07-24 (dayshift) — **CPE-971** landed the first per-family extractor: `media_column::audio_cell` maps
read ID3 tags (via CPE-970) to typed `CellValue`s so Track/Year columns sort numerically. Establishes the
`*_cell -> CellValue` pattern. Remaining: image (a dimensions primitive already exists in `image_preview`) /
video / doc extractors, the column-picker UI, and per-folder persistence.

2026-07-24 (dayshift) — **CPE-974** added the image-family extractor: `image_column::image_dimensions_cell` (header-only read → `CellValue::Dimensions`, sorts by area). With audio (CPE-971) + image, the two commonest per-family extractors are covered. Remaining: video/doc extractors, the column-picker UI (GUI), and per-folder persistence.

2026-07-24 (dayshift) — **CPE-975** added the dispatcher `column_extract::extract_column(ext, bytes, MetaColumn)` — the single seam routing a file to its per-family extractor (audio ID3/FLAC/OGG → typed audio cell; image header → Dimensions). Adding video/doc later is one more arm. Remaining: video/doc extractors, the column-picker UI (GUI), per-folder persistence.

2026-07-25 (workshift) — **CPE-1028 + CPE-1029** added the remaining two per-family extractors:
`video_column::video_cell` (ISO-BMFF `moov/mvhd` header walk → `CellValue::Float` duration seconds,
`mp4`/`mov`/`m4v`) and `doc_column::doc_pages_cell` (pure PDF byte-scan of `/Type /Page` objects,
excluding the `/Pages` tree node → `CellValue::Int` page count). Both wired into
`column_extract::extract_column` via new `MetaColumn::VideoDuration` / `MetaColumn::DocPages` arms with
ext guards, both pure + no new deps, each independently reviewed (opus) + UAT-passed (PR #347, #345). With
audio/image/video/doc all covered, the **per-family extractor layer is complete**. Remaining epic scope:
the column-picker UI (GUI/attended) + per-folder column persistence.

2026-07-25 (workshift) — **CPE-1032** added the per-folder column-config persistence store
(`cpe-server::column_config`: `get`/`set`/`clear` over a `column_config.json` catalog keyed by folder path,
`ServerCtx`-based, tolerant-read, `HeadlessCtx`-tested), mirroring the CPE-836 template store. The details
view can now remember a user's chosen columns per folder. Independently reviewed + UAT-passed (PR #349).
Remaining epic scope: the column-picker UI (GUI/attended) that binds these pieces together.

2026-07-25 (workshift) — **CPE-1039 DONE (PR #356): document-metadata column family.** PDFs now expose
sortable Title/Author/Subject/Keywords/Creator/Producer/Date Created/Date Modified columns via the new
`doc_info_column` extractor wired into `column_extract` (reads CPE-1036's `read_pdf`). Same per-family
`→ CellValue` pattern as `audio_cell`/`image_dimensions_cell`. Independently reviewed + UAT-passed. Next
column candidate: a video-tag family (Title/Artist/… from CPE-1037's `read_mp4`).

2026-07-25 (workshift) — **CPE-1040 DONE (PR #357): video-tag column family.** MP4/MOV videos expose
sortable Title/Artist/Album/Year(numeric)/… columns via `video_tag_column` reading CPE-1037's `read_mp4`.
Both new read codecs (PDF /Info, MP4 tags) are now reachable as columns. Column families shipped this
shift: doc-info (PDF) + video-tag. Remaining 707: the column-picker/persistence UI (attended).

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** Column-picker UI (add/remove/reorder) unbuilt (extractors + per-folder persistence shipped).
