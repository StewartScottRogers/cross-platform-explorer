---
id: CPE-1346
title: "Camera-RAW embedded-preview backend (cpe-server): TIFF/IFD walk → largest embedded JPEG, ZERO new deps"
type: Feature
status: Backlog
priority: Medium
component: cpe-server
tags: [ready]
epic: CPE-102
created: 2026-08-05
closed:
---

## Goal

The **backend** half of Camera-RAW preview support (epic CPE-102), scoped exactly as the ticket says:
**extract and return the embedded JPEG preview** from `.cr2`/`.nef`/`.arw` — NOT demosaicing. Read-only.

## Approach (vetted — see Library `gated-format-readers-dicom-raw-rar-2026-08-05`)

CR2/NEF/ARW are TIFF containers with a full-size JPEG preview embedded in a SubIFD/vendor IFD.
**Zero new dependencies** — a small hand-rolled TIFF/IFD walker (reuse bytes already read; `kamadak-exif`,
already a dep, only as a fallback). New module `crates/server/src/camera_raw.rs`, always compiled (no dep → no
feature gate needed; it's pure logic in the lean-core spirit).

- Parse the 8-byte TIFF header (`II`/`MM` byte order + IFD0 offset). Bounds-check everything; endianness-aware.
- Recursively walk IFDs: each = 2-byte entry count + N×12-byte entries + 4-byte next-IFD offset. Recurse the
  **NextIFD chain** AND pointer entries **SubIFDs (0x014A)** and **ExifIFD (0x8769)**. Guard against cycles /
  runaway offsets (cap IFD count + validate offsets in-bounds) so a malformed file can't loop or panic.
- At each IFD, collect embedded-JPEG candidates two ways: (a) `JPEGInterchangeFormat` (0x0201) +
  `JPEGInterchangeFormatLength` (0x0202); (b) `Compression`==6/7 with `StripOffsets`/`StripByteCounts` whose
  bytes begin with the JPEG SOI `FF D8`. Keep the **largest** candidate by byte length.
- Public surface:
  - `read_raw_preview_data_url(path) -> Result<String, String>` — return the largest embedded JPEG **as-is**
    (no re-encode) as `data:image/jpeg;base64,...`. If no embedded JPEG found, fall back to
    `kamadak-exif`'s standard thumbnail; if still none → `Err` (frontend shows metadata).
  - (optional) `read_raw_meta(path)` for a few EXIF tags if trivial via the same walk — nice-to-have, not required.
- Corrupt/short/non-TIFF input → `Err`, never panic or hang.

## Acceptance criteria

- `read_raw_preview_data_url` returns a `data:image/jpeg;base64,...` for a fixture; picks the LARGEST embedded
  JPEG when several exist (incl. one reached only via a SubIFD 0x014A pointer).
- Unit tests build **synthetic minimal TIFF+IFD+embedded-JPEG byte blobs** (both `II` and `MM` byte orders;
  a SubIFD-nested preview; a "no preview" case → Err/fallback). Malformed/truncated/cyclic-offset input
  returns `Err` and NEVER panics (add to the panic-safety expectations).
- **Zero new dependencies** (verify `Cargo.toml` unchanged / `cargo tree` identical). `cargo test` +
  `cargo clippy --all-targets -- -D warnings` (both feature modes) green.

## Notes

Backend-only (no command/frontend wiring — follow-up). Standalone new module → parallelizable with
CPE-1345/1347. Consider adding `read_raw_preview_data_url` to the parser panic-safety battery.
