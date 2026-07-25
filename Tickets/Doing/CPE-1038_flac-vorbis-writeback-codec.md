---
id: CPE-1038
title: FLAC / Vorbis-comment write-back codec
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-725
estimate: 2-3h
---

## Summary
Second **write-back** codec for the media-metadata studio (epic CPE-725), after CPE-1035 (ID3v2). Lets
the studio write edited tags back into **FLAC** files. Extends `crates/server/src/media_meta_write.rs`:
- `pub fn write_vorbis_comment(fields: &[MetaField]) -> Vec<u8>` — build a raw Vorbis-comment block
  (vendor string + `group == "vorbis"` fields as `KEY=value` UTF-8 entries, all little-endian lengths),
  the inverse of `media_meta_read::parse_vorbis_comment`.
- `pub fn write_flac(orig: &[u8], fields: &[MetaField]) -> Vec<u8>` — rebuild a FLAC stream with its
  `VORBIS_COMMENT` metadata block (type 4) replaced by the freshly-built one (inserted after STREAMINFO
  if absent), **preserving** STREAMINFO (type 0, must stay first) and all other metadata blocks and the
  audio frames byte-for-byte, and fixing the last-metadata-block flag.

**Out of scope (separate ticket):** OGG write-back — rewriting an OGG stream needs full repaging + CRC32
recomputation + segment-table rebuild, materially riskier; leave it for a dedicated ticket.

## Acceptance Criteria
- [ ] `write_flac(orig, fields)` output, read back through `media_meta_read::read_flac`, yields the
      edited fields (round-trip: read→edit via `apply_edits`→write→read == expected). Cover: a FLAC with
      an existing VORBIS_COMMENT (replaced, not duplicated) and one without (block inserted after
      STREAMINFO).
- [ ] STREAMINFO block + any other metadata blocks + the audio frames after the metadata are preserved
      byte-for-byte; the last-metadata-block flag is correct in the output; writing twice is idempotent.
- [ ] `write_vorbis_comment` round-trips through `parse_vorbis_comment` for representative fields.
- [ ] Never panics on empty/short/garbage `orig` (returns `orig` unchanged or an empty-safe result);
      pure `std`, **no new deps**; `cargo test -p cpe-server` green; `cargo clippy -p cpe-server
      --all-targets -D warnings` clean in **both** feature modes.

## Work Log
2026-07-25 (workshift) — Filed + dispatched after CPE-1035 merged (media_meta_write.rs now on main).
Reuses the read side (CPE-972 read_flac / parse_vorbis_comment). OGG write-back deferred to its own
ticket (repaging complexity).
