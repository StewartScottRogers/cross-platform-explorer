---
id: CPE-1290
title: "IPTC (JPEG APP13 / 8BIM) read codec"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-725
---

## Summary
`media_meta::read_all` reads EXIF for JPEG but not IPTC (the caption/keywords/by-line block many photos
carry in a JPEG APP13 "Photoshop 3.0" 8BIM segment). Add an IPTC read codec and merge its fields into the
JPEG read path. Headless, cargo-tested.

## Build
- New `pub fn read_iptc(bytes: &[u8]) -> Vec<MetaField>` in `crates/server/src/media_meta_read.rs`: locate
  the JPEG APP13 marker (`0xFF 0xED`) carrying the `Photoshop 3.0\0` 8BIM signature, find the IPTC IIM
  resource block (8BIM id `0x0404`), and parse IIM datasets → friendly `MetaField`s: Caption/Abstract
  (2:120), Keywords (2:25, repeatable), By-line (2:80), Headline (2:105), Copyright Notice (2:116),
  City (2:90), Country (2:101). Bounds-checked and never-panic, mirroring the existing codecs' defensive
  parsing (no slice-at-arbitrary-offset panics).
- Wire it into `media_meta::read_all` for `"jpg" | "jpeg"` by MERGING with `read_exif`'s output (both can be
  present; append IPTC fields — dedup only if trivially identical).
- No new dependency (byte parsing). Use the same friendly key names style as the ID3/EXIF codecs.

## Acceptance criteria
- `read_iptc` extracts Caption/Keywords/By-line/etc. from a crafted APP13/8BIM/IIM fixture; `read_all("jpg",
  …)` returns EXIF + IPTC fields together; a JPEG with no APP13 yields no IPTC fields (and no panic);
  garbage bytes → empty, no panic.
- `cargo test -p cpe-server` green; `cargo clippy` clean both feature modes; no new dep.

## Notes
Epic CPE-725. Shares `media_meta_read.rs` + `media_meta.rs` (read_all arm) with CPE-1291 (XMP read) +
CPE-1292 (WAV read) → sequence those after this.

## Work Log
- 2026-08-03 — read_iptc merged (#586). Reviewer APPROVE, 8BIM-walk correct (even-padded Pascal name), 7 IIM fields spec-correct, panic-safe (bounds-checked + real truncation fuzz), read_all merges exif+iptc, 1389 re-run green clippy clean.
