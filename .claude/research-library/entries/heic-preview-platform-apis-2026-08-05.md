---
title: "How to decode HEIC/HEIF previews via per-OS platform APIs (no libheif) — build plan"
date: 2026-08-05
tags: [heic, heif, cpe-097, preview, wic, imageio, windows-rs, objc2, platform-api, no-libheif, licensing]
status: current
---

## Question
User chose the **per-OS platform-API** approach for HEIC/HEIF preview (CPE-097) over bundling libheif.
How to build it: which crates/APIs, licensing, caveats, testability?

## Decision & plan (researched 2026-08-05)
New module `src-tauri/src/heic_preview.rs` with `pub fn decode_heic_preview(path) -> Result<String,String>`
returning `data:image/png;base64,...`. Dispatch by platform `cfg` (NO Cargo feature — mirror the existing
per-OS dep blocks). Thin `#[tauri::command] async fn read_heic_preview_data_url` in lib.rs `spawn_blocking`s
into it (after `ensure_previewable_size`). Register in BOTH `generate_handler!` sites + regen bindings.

### Windows (WIC) — headless-verifiable ON THIS MACHINE
- `windows` crate is **already a dep** (`0.56`, MIT/Apache) — just ADD features to the existing entry:
  `Win32_Graphics_Imaging` + `Win32_System_Com` (keep the current features). Do NOT add a 2nd windows dep.
- Flow: `CoInitializeEx(MULTITHREADED)` → `CoCreateInstance(CLSID_WICImagingFactory)` →
  `CreateDecoderFromFilename(GENERIC_READ, CacheOnDemand)` → `GetFrame(0)` → `CreateFormatConverter` →
  `Initialize(GUID_WICPixelFormat32bppRGBA)` → `GetSize` → `CopyPixels(stride=w*4)` → `(w,h,rgba)`; then
  `CoUninitialize`. Wrap ALL calls → `Err(String)`, never panic.
- **CAVEAT**: WIC decodes HEIC only if the Store "HEIF Image Extensions" (+ "HEVC Video Extensions") are
  installed — NOT default on many installs. No redistributable fallback, no query API — just try+catch.
  `CreateDecoderFromFilename` fails with WINCODEC_ERR_COMPONENTNOTFOUND (0x88982F50) family when absent →
  return a clean `Err` (UI falls back to metadata; optionally hint "install HEIF Image Extensions").
  **This dev machine HAS them installed** (Microsoft.HEIFImageExtension 1.2.36.0 + HEVCVideoExtension
  2.5.10.0, confirmed via Get-AppxPackage) → a REAL decode is verifiable here, not just the Err path.

### macOS (ImageIO) — cfg-gated, CI-compiled, visual attended
- Crates (all `Zlib OR Apache-2.0 OR MIT`, pure bindings, no bundled C): `objc2` 0.6, `objc2-foundation`
  0.3, `objc2-core-foundation` 0.3, `objc2-core-graphics` 0.3, `objc2-image-io` 0.3 — in a
  `[target.'cfg(target_os="macos")'.dependencies]` block (mirrors existing per-OS blocks).
- Flow: `CFURL::from_file_system_representation` → `CGImageSourceCreateWithURL` →
  `CGImageSourceCreateImageAtIndex(0)` (the decode; macOS has native HEIC since 10.13, no install needed) →
  `CGBitmapContextCreateWithData(RGBA, PremultipliedLast)` → `CGContextDrawImage` → copy bytes → PNG.
  Any None/err → `Err`. CI's macOS leg compiles+unit-tests it; a human on a Mac confirms visual correctness.

### Shared / structure
- Add ONE pure fn to cpe-server: `image_preview::encode_rgba_to_png_data_url(w,h,rgba) -> Result<String>`
  (reuse the existing `RgbaImage::from_raw`→`write_to(Png)`→base64 pattern) — both platform decoders feed it,
  keeping domain logic in cpe-server and only the FFI in src-tauri.
- Non-win/non-mac fallback: `Err("HEIC preview not supported on this platform")` (Linux has no clean path).
- Licensing: all permissive; the actual HEVC codec is the OS's own system component (never linked/shipped).
  Binary size: tens of KB (bindings only; system frameworks dynamically linked).

### Testability
- Headless HERE: compiles + clippy clean; corrupt/oversized `.heic` → `Err` (no panic); **and a real
  `.heic`→PNG decode round-trips** (codec present on this box) — assert URL is `data:image/png;base64,` and
  base64 payload loads via `image::load_from_memory` to non-zero dims. Write the test to greenly handle BOTH
  codec-present and codec-absent CI (don't ASSERT success — assert "no panic; if Ok, URL well-formed").
- Attended: the macOS ImageIO visual correctness (real Mac).

## Bottom line
Fully buildable now. Windows path is the headless-verifiable deliverable (codec present on this machine);
macOS path is cfg-gated + CI-compiled with an attended visual check. Tickets: CPE-1351 (HEIC backend +
provider wiring). Supersedes the old libheif blocker on CPE-097. See [[gated-format-readers-dicom-raw-rar-2026-08-05]].
