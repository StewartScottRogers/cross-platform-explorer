---
title: "Which of the gated format-reader epics (HEIC/DICOM/RAR/camera-RAW) can be built license-clean + headless?"
date: 2026-08-05
tags: [format-readers, preview, cpe-097, cpe-102, cpe-111, cpe-219, dicom, camera-raw, rar, heic, licensing, feature-gate, no-new-deps, signed-binary]
status: current
---

## Question
The four long-Blocked "preview provider" epics (HEIC CPE-097, DICOM CPE-219, RAR CPE-111, camera-RAW CPE-102)
were parked as "needs a heavy/licensed dep". Which can actually be built autonomously — license-clean for a
**code-signed, redistributed** binary, pure-Rust, cargo-testable headless — and which genuinely need the user?

## Findings (vetted 2026-08-05 via crates.io license API + context7)
| Format | Approach | License | New deps | Verdict |
|---|---|---|---|---|
| **DICOM** (CPE-219) | `dicom-object` (tags) + `dicom-pixeldata` (pixel→image), decode frame 0 + window/level → PNG | **MIT OR Apache-2.0** (whole `dicom-rs` family) | yes, feature-gated | **GO** |
| **camera-RAW** (CPE-102) | hand-rolled TIFF/IFD walk to extract the **largest embedded JPEG** (scope = embedded preview, NOT demosaic); `kamadak-exif` (already a dep) as fallback | none new | **zero** | **GO** |
| **RAR** (CPE-111) | hand-rolled RAR4/RAR5 **header walk** to LIST entries (no decompression → no UnRAR) | none new | **zero** | **GO** |
| **HEIC** (CPE-097) | — | both pure-Rust decoders (`imazen/heic`, `ente-io/heic-decoder`) are **AGPL-3.0**; `libheif-rs` = native-C LGPL | — | **DEFER (user)** |

### DICOM details
- Crates `dicom-object` + `dicom-pixeldata`, all `MIT OR Apache-2.0`. Dep-tree moderate (~comparable to parquet/rusqlite already present).
- **Key**: `dicom-pixeldata` default features `["rayon","native"]` where `native = ["jpeg","rle","deflate"]` are all **pure-Rust**. The native-codec pulls (`openjp2` JPEG2000, `charls` JPEG-LS, `gdcm-rs`) are **separate opt-in features, off by default** — so with default features the reader is pure-Rust and JPEG2000/JPEG-LS transfer syntaxes decode-error *gracefully* (exactly the ticket's fallback).
- API: `open_file` → `element_by_name("PatientName")?.to_str()`; `decode_pixel_data()?.to_dynamic_image(0)` (+ `.window(...)` for window/level) → `image::save PNG`.
- Feature gate `dicom-thumb`, deps `optional=true`. Test headless by CONSTRUCTING a minimal uncompressed DICOM object in-memory, writing to bytes, reading back + decoding (round-trip) — no real clinical file needed.

### camera-RAW details
- CR2/NEF/ARW are TIFF containers. `kamadak-exif` only models the fixed PRIMARY/THUMBNAIL IFD pair — it MISSES the SubIFD(0x014A)/vendor-preview IFDs where the *largest* preview lives. So reuse it only as a fallback.
- Do a small recursive TIFF-IFD walk (8-byte header → byte order + IFD0 offset; each IFD = 2-byte count + N×12-byte entries + next-IFD offset; recurse NextIFD chain AND SubIFD 0x014A / ExifIFD 0x8769 pointers). Collect JPEG candidates via `JPEGInterchangeFormat`(0x201)+`Length`(0x202) OR `Compression==6/7`+`StripOffsets`/`StripByteCounts` pointing at `FFD8`. Return the largest raw JPEG **as-is** (already valid, no re-encode). Quirks: CR2 = 3rd IFD in main chain; NEF/ARW = SubIFD off IFD0 (sometimes double-nested). ~150-300 LOC, synthetic-fixture testable.

### RAR details
- Reject the `unrar` crate (wraps native C, non-free) AND the pure-Rust `rar` crate (MIT but thinly-maintained: 2 releases, ~8k downloads, and pulls aes/cbc/hmac/pbkdf2/sha2 for full extraction we don't need — needless attack surface on untrusted input).
- Hand-roll a listing-only walk (mirror `archive.rs`'s zip/tar/7z/iso dispatch style):
  - RAR4: marker `Rar!\x1a\x07\x00` → blocks with common header (crc16,type,flags,head_size); file blocks type `0x74` carry pack_size/unp_size/name inline; skip pack_size to next block.
  - RAR5: marker `Rar!\x1a\x07\x01\x00` → **vint**-encoded headers; file records type `2` carry sizes + UTF-8 name; same skip-by-size walk.
  - No CRC/decompression/dictionary needed for listing. Build tiny hand-crafted RAR4/RAR5 blobs in tests.

## Bottom line
Build DICOM (feature-gated), camera-RAW embedded-preview (zero-dep), RAR listing (zero-dep) now — backend
modules first (pure, cargo-tested), wiring (commands + frontend provider registry) as a follow-up. DEFER HEIC
to the user as a native-dep/platform-API decision (no permissive pure-Rust path exists — both are AGPL).
The **backend decode** is fully headless/CI-verifiable; only the *visual* judging of decoded images is attended
(cover via gui-smoke/Visual-Critic later). See tickets CPE-1345 (DICOM) / CPE-1346 (RAW) / CPE-1347 (RAR).
