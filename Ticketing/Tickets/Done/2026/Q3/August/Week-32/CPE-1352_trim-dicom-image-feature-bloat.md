---
id: CPE-1352
title: "Perf: trim DICOM ship-cost — drop dicom-pixeldata's image feature, encode PNG from raw pixels"
type: Task
status: Done
priority: Low
component: cpe-server
tags: [ready]
epic: CPE-219
created: 2026-08-05
closed: 2026-08-05
---

## Problem (Performance Guard)

Shipping DICOM (CPE-1350) added **+2.81 MiB (~8.2%)** to the release binary. Per the #645 supply-chain
review, this is NOT the AVIF/JPEG-XL horror the original PR text claimed — `cargo tree`/filesystem checks
confirmed **rav1e/ravif/jxl-oxide are unenabled Cargo.lock rows, never compiled, 0 shipped bytes**. The real
added footprint is: the 7 necessary `dicom-*` crates + **`exr` and `pnm`** decoders pulled into the app's
`image` dep purely by `dicom-pixeldata`'s `default-features = true` `image` feature (feature-unification
expands `image` past the app's curated `tiff/png/jpeg/gif/webp/bmp` set) + DICOM's CJK charset tables
(`encoding` + `encoding-index-*`) + a duplicate `itertools`.

## Trim (feasible per the #645 review)

`dicom-pixeldata`'s `DecodedPixelData` exposes `to_vec_with_options::<u8>(&ConvertOptions{ voi_lut, ... })`
plus `rows()/columns()/samples_per_pixel()/photometric_interpretation()` accessors — **all un-feature-gated**.
So the reader can obtain windowed 8-bit pixel bytes WITHOUT `dicom-pixeldata`'s `image` integration:

- Set `dicom-pixeldata = { ..., default-features = false, features = ["rayon", "native"] }` (drop `image`) in
  `crates/server/Cargo.toml`.
- In `crates/server/src/dicom.rs::read_dicom_image_data_url`, replace `to_dynamic_image(0)` with
  `to_vec_with_options` → build an `image::ImageBuffer`/`DynamicImage` from the raw bytes using the app's
  OWN already-curated `image` dep → encode PNG (reuse the existing `read_image_data_url` encode path, or the
  `encode_rgba_to_png_data_url` helper if CPE-1351 added it). Handle the common photometric interpretations
  (MONOCHROME1/2, RGB) + samples-per-pixel; unsupported → clean `Err` (as today).
- Confirm this drops `exr`/`pnm` (and ideally the duplicate `itertools`) from the app build; re-measure the
  size delta (target: shrink toward the ~7-dicom-crates minimum).

## Acceptance criteria

- DICOM images still decode + render correctly (the 5 dicom unit tests still pass, incl. the PNG-dimensions
  test); window/level still applied.
- `cargo tree` for the app no longer shows `exr`/`pnm` pulled via dicom-pixeldata; measured binary-size delta
  is smaller than the +2.81 MiB baseline (report the new number).
- clippy both modes green; no behavior regression.

## Notes

Optional cleanup, low priority — DICOM already ships and works (CPE-1350). Also worth a one-line note: the
`encoding`/`encoding-index-*` crates (unmaintained rust-encoding) come transitively from `dicom-encoding`;
not removable without upstream change, narrow attack surface (static tables) — accept + document.

## Work Log
- 2026-08-05 (workshift): PR #647 merged. Dropped dicom-pixeldata image feature (default-features=false, features=[rayon,native]); rebuilt read_dicom_image_data_url on raw-pixel path (to_vec_frame_with_options + accessors); exr + 7 transitive crates gone (315->307). Window/level parity traced (VoiLutOption::First is REQUIRED in the raw path); MONOCHROME1 hand-inversion; 16-bit narrow. Reviewer CHANGES-then-APPROVE (caught + fixed a silent YBR_FULL wrong-color regression — YCbCr->RGB ported verbatim from dicom-pixeldata) + UAT PASS (pixel-exact MONOCHROME2/1). NOTE: upstream dicom-pixeldata has a G-term sign bug we now inherit bug-for-bug (== pre-trim main); follow-up CPE-1353 fixes it (we own the fn now).
