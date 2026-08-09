---
id: CPE-1345
title: "DICOM backend reader (cpe-server): tags + frame→PNG, feature-gated dicom-thumb (pure-Rust dicom-rs)"
type: Feature
status: Done
priority: Medium
component: cpe-server
tags: [ready]
epic: CPE-219
created: 2026-08-05
closed: 2026-08-05
---

## Goal

The **backend** half of DICOM preview support (epic CPE-219): a pure `cpe-server` module that reads a
`.dcm` file's key tags and decodes its first frame to a PNG, so a later frontend provider can show
"image + key tags". Read-only. Backend decode is fully cargo/CI-verifiable; the *visual* judging is attended
(covered later via gui-smoke).

## Approach (vetted — see Library `gated-format-readers-dicom-raw-rar-2026-08-05`)

- Add `dicom-object` + `dicom-pixeldata` as **`optional = true`** deps (both `MIT OR Apache-2.0`, pure-Rust).
  Use `dicom-pixeldata` **default features only** (`rayon` + `native` = pure-Rust jpeg/rle/deflate). Do NOT
  enable `openjp2`/`charls`/`gdcm` (those are native C — must stay off).
- New Cargo feature **`dicom-thumb`** gating a new module `crates/server/src/dicom.rs`. OFF by default so the
  lean build compiles zero DICOM code / pulls no dicom deps (the established "small when off" rule — mirror
  `pdf-thumb`).
- Public surface (behind the gate):
  - `read_dicom_tags(path) -> Result<Vec<(String,String)>, String>` — a curated set (PatientName, StudyDate,
    Modality, Rows, Columns, SeriesDescription, etc.), missing tags skipped gracefully.
  - `read_dicom_image_data_url(path) -> Result<String, String>` — `open_file` → `decode_pixel_data` →
    frame 0 → apply window/level (use the pixel data's window center/width if present) → encode PNG →
    return `data:image/png;base64,...` (mirror `image_preview::read_image_data_url`).
- **Graceful fallback**: a compressed transfer syntax needing a native codec (JPEG2000/JPEG-LS/GDCM) must
  return a clean `Err(...)` (so the frontend falls back to the tag view), NEVER panic or hang. Corrupt/short
  input → `Err`, never panic.

## Acceptance criteria

- `read_dicom_tags` + `read_dicom_image_data_url` behind `#[cfg(feature="dicom-thumb")]`; default build has
  zero DICOM code (verify `cargo build` default has no dicom crates via `cargo tree`).
- Unit tests (under the feature) that **construct a minimal uncompressed DICOM object in-memory** (a few
  tags + a tiny pixel array), write to bytes, then read back: tags read correctly and the image decodes to a
  PNG data URL of the right dimensions. A corrupt/truncated buffer returns `Err` (no panic).
- `cargo test --features dicom-thumb` green; `cargo clippy --all-targets --features dicom-thumb -- -D warnings`
  green; **and** the DEFAULT `cargo clippy --all-targets -- -D warnings` still green (module absent when off).
- No `openjp2`/`charls`/`gdcm` in the tree. Only `Cargo.toml` + the new `dicom.rs` touched.

## Notes

Backend-only this ticket (no command/frontend wiring — that's a follow-up integration ticket, to avoid
shared-file collisions). Standalone new module → parallelizable with CPE-1346/1347. If `dicom.rs` types are
NOT specta-exported, no bindings regen; if any are, regenerate `bindings.gen.ts`.

## Work Log
- 2026-08-05 (sprint): DICOM reader (dicom-rs, feature-gated dicom-thumb, native codecs off). PR #642 squash-merged to main (3a994f5a). Worker(sonnet); independent Reviewer(+security lens) APPROVE + UAT PASS. Backend-only (no command/frontend wiring — follow-up). main compiles clean both feature modes.
