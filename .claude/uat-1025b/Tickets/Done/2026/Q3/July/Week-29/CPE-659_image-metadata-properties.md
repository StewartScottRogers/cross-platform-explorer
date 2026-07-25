---
id: CPE-659
title: Image dimensions + EXIF in Properties
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-615
estimate: 2-3h
---

## Summary
Final CPE-615 (media gallery) DoD gate: the Properties dialog shows **dimensions + basic EXIF** for
image files. Adds a backend `image_meta(path)` command (dimensions via the `image` crate, EXIF via
`kamadak-exif`) and renders width×height, camera, date-taken, ISO, aperture, exposure, and focal
length in `PropertiesDialog` when a single image is selected. Also aligns the `image` crate features
with the epic's documented v1 decision (JPEG/GIF/WebP/BMP were never enabled — so real-photo
thumbnails silently didn't work), which retroactively fixes JPEG/GIF/WebP thumbnails.

## Acceptance Criteria
- [x] Backend `image_meta(path) -> ImageMeta` returns best-effort width/height + EXIF fields; missing
      data is `None`, never an error (cargo-tested).
- [x] `image` crate features include jpeg/gif/webp/bmp so dimensions + thumbnails cover real photos.
- [x] `PropertiesDialog` auto-loads `image_meta` for a single image file and shows the present fields.
- [x] i18n labels for the new rows added to all 12 locales.
- [x] `npm run check` clean; cargo test + suite green.

## Work Log
2026-07-18 (nightshift) — Picked up as the last CPE-615 DoD gate. Estimate 2-3h.

## Resolution
Added backend `image_meta(path) -> ImageMeta` (src-tauri/src/lib.rs): dimensions via
`image::image_dimensions` with an EXIF `PixelXDimension` fallback for JPEGs, plus camera/lens/date-taken/
ISO/aperture/exposure/focal-length read via the new `kamadak-exif` crate. Best-effort — every field is
optional and a non-image returns an all-`None` struct, never an error (2 cargo tests). Enabled
jpeg/gif/webp/bmp on the `image` crate to match the epic's documented v1 format decision, which also
fixes real-photo thumbnails. Wired into `PropertiesDialog.svelte` (auto-loads for a single image, renders
present rows), 8 label keys added to all 12 locales, docs updated (03-explorer.md). This closes the last
CPE-615 DoD gate. Files: Cargo.toml, Cargo.lock, lib.rs, PropertiesDialog.svelte, i18n.ts, 03-explorer.md.
