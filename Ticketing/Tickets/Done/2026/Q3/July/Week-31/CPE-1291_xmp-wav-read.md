---
id: CPE-1291
title: "XMP + WAV/RIFF-INFO read codecs"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-725
---

## Summary
Two more read codecs for `media_meta`, batched into one ticket because they share `media_meta_read.rs` +
the `media_meta::read_all` dispatch (so one worker does both sequentially, avoiding a self-conflict): XMP
packet reading (photos) and WAV/RIFF-INFO reading (audio). Headless, cargo-tested.

## Build (both, in `crates/server/src/media_meta_read.rs` + `media_meta.rs`)
- **XMP** — `pub fn read_xmp(bytes: &[u8]) -> Vec<MetaField>`: locate the XMP packet — in a JPEG APP1
  segment whose payload begins `http://ns.adobe.com/xap/1.0/\0`, OR a standalone `.xmp` sidecar's bytes —
  bounded to the `<?xpacket …>` … `</x:xmpmeta>` (or `</rdf:RDF>`) region, and extract common Dublin-Core /
  XMP props with a small TOLERANT tag scan (NO full XML parser, NO new dep): `dc:title`, `dc:creator`,
  `dc:subject` (keywords), `dc:description`, `xmp:CreateDate`, `photoshop:Headline`. Handle both attribute
  form (`prop="value"`) and element form (`<prop>value</prop>`), and `rdf:Bag`/`rdf:Seq/rdf:li` lists.
  Wire into `read_all` for `jpg`/`jpeg` (merge with the existing EXIF + IPTC fields) and add an `xmp`
  extension arm that returns `read_xmp` for a standalone sidecar.
- **WAV** — `pub fn read_wav(bytes: &[u8]) -> Vec<MetaField>`: walk the RIFF chunk tree (`RIFF`…`WAVE`) to
  the `LIST`/`INFO` chunk and read `INAM`(Title)/`IART`(Artist)/`IPRD`(Album)/`ICRD`(Year)/`ICMT`(Comment)/
  `IGNR`(Genre), mapped to the SAME friendly keys the ID3/Vorbis codecs use (so the audio columns handle
  WAV unchanged). Wire `"wav"` into `read_all`, and add `"wav"` to `AUDIO_EXTS` in
  `crates/server/src/column_extract.rs` so it also feeds the audio metadata columns.
- BOUNDS-CHECK every read (the parser-audit rule: never slice at an unchecked offset). Never panic. No new
  dep. UTF-8 lossy for values.

## Acceptance criteria
- `read_xmp` extracts title/creator/subject/etc. from a crafted XMP packet (both a JPEG-embedded APP1 and a
  standalone `.xmp` fixture); `read_all("jpg", …)` now returns EXIF + IPTC + XMP together.
- `read_wav` extracts INAM/IART/etc. from a crafted WAV INFO fixture; `read_all("wav", …)` returns them;
  `"wav"` is in `AUDIO_EXTS`.
- Truncated/garbage input → empty, no panic, for both. `cargo test -p cpe-server` green; `cargo clippy`
  clean both feature modes; no new dep.

## Notes
Epic CPE-725. Do XMP then WAV in the same worker (shared file). Sequenced after CPE-1290 (IPTC, merged).

## Work Log
- 2026-08-03 — XMP + WAV read codecs merged (#590, landed via ff after media_meta.rs union resolve with CPE-1288). 1 rework: reviewer required wiring read_xmp/read_wav into the parser_panic_safety.rs fuzz harness (CPE-1169, overflowing-length-field class) — added read_xmp/read_wav_never_panics + table-driven dispatch variants (harness 27->29), avoided the hollow-test trap (full 12B WAV header). No functional bug (offset-audited panic-safe). 1410 green clippy clean.
