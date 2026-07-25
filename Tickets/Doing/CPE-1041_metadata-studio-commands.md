---
id: CPE-1041
title: Metadata Studio command layer (read all / write edits back)
type: feature
component: Multiple
priority: high
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-725
estimate: 2-3h
---

## Summary
The Metadata Studio dialog (next slice) needs a backend to call — today every read/write codec lives in
`cpe-server` but **none is exposed as a Tauri command**. This slice adds the command layer:

- New dispatcher `crates/server/src/media_meta.rs`:
  - `read_all(ext, bytes) -> Vec<MetaField>` — all a file's metadata fields, routing by extension across
    the read codecs (mp3→ID3, flac→FLAC, ogg/oga→OGG, pdf→PDF /Info, mp4/mov/m4v→video tags,
    jpg/jpeg/tif/tiff→EXIF).
  - `is_writable(ext) -> bool` — true where a write codec exists today (mp3, flac).
  - `write_back(ext, orig, edits) -> Result<Vec<u8>, String>` — `read_all` → `apply_edits` →
    `write_id3v2`/`write_flac`; friendly `Err` for formats without a writer yet.
- `MetaEdit` gains `serde::Deserialize` (it's now a command input).
- Three thin async Tauri commands in `src-tauri/src/lib.rs` (registered in **both** `generate_handler!`
  and `collect_commands!`, then bindings regenerated per the CPE-968 lesson):
  - `metadata_read(path) -> Vec<MetaField>`
  - `metadata_writable(path) -> bool`
  - `metadata_write(path, edits: Vec<MetaEdit>) -> Vec<MetaField>` — reads the file, applies edits, writes
    back **atomically** (temp file + rename), returns the re-read fields.

## Acceptance Criteria
- [ ] `read_all` routes each extension to the right codec (unit-tested); unknown ext → empty.
- [ ] `write_back` round-trips for mp3 + flac (build tag → edit → write_back → read_all == edited),
      preserves audio; unsupported ext → `Err`.
- [ ] The three commands compile, are registered in both macros, and appear in regenerated
      `src/lib/bindings.gen.ts`; `metadata_write` writes atomically (never truncates the original on a
      mid-write failure).
- [ ] `cargo test -p cpe-server` green; clippy clean both modes; `npm run check` passes.

## Work Log
2026-07-25 — Filed (user present, directing the Metadata Studio build). First of three slices: commands →
dialog → GUI verify. Only mp3/flac are writable today; other formats are read-only in the studio until
their write codecs land (OGG/EXIF/video/PDF write-back are separate, deferred as risky).
