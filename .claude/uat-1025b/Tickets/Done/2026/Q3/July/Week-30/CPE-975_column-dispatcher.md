---
id: CPE-975
title: Metadata-column dispatcher (bytes + ext → CellValue)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-707
estimate: 1h
---

## Summary
The capstone that unifies CPE-707's per-family extractors into one entry point. Adds
`cpe-server::column_extract::extract_column(ext, bytes, MetaColumn) -> CellValue`, routing a file's bytes +
extension to the right extractor — the audio read codecs (CPE-970/972/973: ID3/FLAC/OGG) → audio typing
(CPE-971), and the image header reader (CPE-974). The column UI / an MCP tool / a command calls this one
function; a kind mismatch yields `Empty` (sorts last).

Dayshift tier-4 (2026-07-24), integrating the read-codec + extractor arc landed today.

## Design
- `MetaColumn` enum: `Audio(AudioColumn)` | `ImageDimensions`.
- `read_audio_tags(ext, bytes)` dispatches by extension: `mp3` → `read_id3v2`, `flac` → `read_flac`,
  `ogg`/`oga` → `read_ogg`; non-audio ext → no fields. Case-insensitive.
- `extract_column` runs the audio fields through `media_column::audio_cell`, or gates image bytes by an
  image extension before `image_column::image_dimensions_cell`; any mismatch → `CellValue::Empty`.
- Pure over `(ext, bytes)`; the adapter reads the file's leading bytes.

## Acceptance Criteria
- [x] Audio columns route to the codec matching the extension (mp3/flac/ogg), case-insensitively, and
      produce the typed cell; image Dimensions gate by image extension.
- [x] A kind mismatch (audio column on an image, unknown extension, Dimensions on a text file) → `Empty`.
- [x] Cargo-tested (4 tests, compact inline fixtures); clippy `--all-targets -D warnings` clean both modes;
      no new deps.

## Notes
- One seam for the column system: adding a video/doc extractor later is one more `MetaColumn` arm + ext
  mapping. The column-picker UI + per-folder persistence remain (GUI) in CPE-707.

## Work Log
- 2026-07-24 (dayshift) — Built `column_extract`, tying together today's CPE-970–974. 4 tests, clippy clean.
