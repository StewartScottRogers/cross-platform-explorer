---
id: CPE-1305
title: IPTC + XMP metadata write-back codecs (JPEG APP13/APP1)
type: feature
component: Backend
priority: medium
tags: ready
created: 2026-08-03
epic: CPE-725
estimate: 3-4h
---

## Summary
Child of CPE-725 (Media Metadata Studio). The Studio can already READ IPTC and XMP (`media_meta_read.rs`
`read_iptc`/`read_xmp`, groups `"iptc"`/`"xmp"`) but cannot WRITE them: `is_writable()`
(`crates/server/src/media_meta.rs:43`) lists only `mp3|flac|jpg|jpeg|ogg|oga|wav|pdf`, and
`media_meta_write.rs` has ID3/FLAC-Vorbis/EXIF/OGG/WAV/PDF codecs but no `write_iptc`/`write_xmp`.

Add write-back for **IPTC** (JPEG APP13 Photoshop 8BIM IRB) and **XMP** (XMP packet, JPEG APP1
`http://ns.adobe.com/xap/1.0/`). The Studio UI needs NO change — `MetadataStudioDialog.svelte` derives
its editable tabs from the backend `group`s + `metadataWritable()`, exactly the mechanism that lit up
EXIF/OGG/WAV/PDF autonomously.

## Acceptance Criteria
- [ ] `write_iptc` writes/updates IPTC IIM fields inside a JPEG APP13 8BIM "Photoshop 3.0" IRB, inserting
      the segment if absent and replacing it (preserving other segments incl. existing EXIF APP1) if present.
- [ ] `write_xmp` writes/updates the XMP packet in a JPEG APP1 `xap` segment, insert-or-replace, other
      segments preserved.
- [ ] `is_writable()` / `write_back()` dispatch in `media_meta.rs` extended to route IPTC + XMP.
- [ ] **Round-trip `cargo test`**: for each codec, write a known field into a fixture JPEG, then read it
      back via the existing `read_iptc`/`read_xmp` and assert equality; assert other metadata (EXIF) and the
      image data survive untouched. Follow the existing round-trip tests for EXIF/PDF as the template.
- [ ] `clippy --all-targets -D warnings` clean in BOTH feature modes; existing suite still green.
- [ ] If any `specta::Type` struct changes, regenerate `bindings.gen.ts` (drift guard) — likely none here.

## Notes
- Mirror the existing `write_exif` JPEG-segment machinery (APP1 length fields, segment walk, insert vs
  replace) — IPTC (APP13) and XMP (APP1/xap) are siblings of it. Do NOT hand-roll a whole new JPEG parser
  if the existing EXIF path already walks segments; reuse it.
- Prefer no new dependencies. If a well-scoped, already-vendored crate handles 8BIM/XMP, use it; otherwise
  the segment surgery is a bounded hand-roll (that's why this is opus-tier).
- Video (`write_mp4` atom rewriting) is explicitly OUT of scope — riskier, a separate later ticket.

## Work Log
2026-08-03 (sprint) — Filed by the Foreman from the epic-survey researcher (grep-verified: no
`write_iptc`/`write_xmp` exist; read side already landed; UI auto-derives). Dispatched to an opus worker.
