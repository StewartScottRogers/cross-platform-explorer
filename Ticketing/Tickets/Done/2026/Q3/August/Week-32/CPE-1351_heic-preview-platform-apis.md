---
id: CPE-1351
title: "HEIC/HEIF preview via per-OS platform APIs (Windows WIC + macOS ImageIO) — backend + provider wiring"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-097
created: 2026-08-05
closed: 2026-08-05
---

## Goal

Decode `.heic`/`.heif` to a viewable image in the preview pane using **per-OS platform APIs** (user-approved:
Windows WIC / macOS ImageIO — NOT libheif, NOT a pure-Rust decoder). Full plan in Library
`heic-preview-platform-apis-2026-08-05`. Read-only. NO bundled C lib, all-permissive licenses, plain
platform `cfg` (no Cargo feature).

## Backend

1. New pure fn in cpe-server: `image_preview::encode_rgba_to_png_data_url(w:u32,h:u32,rgba:Vec<u8>) ->
   Result<String,String>` (reuse the existing `RgbaImage::from_raw`→`write_to(Png)`→base64 pattern in
   `read_image_data_url`). Cargo-tested. Both platform decoders feed this.
2. New module `src-tauri/src/heic_preview.rs` with `pub fn decode_heic_preview(path:&str) ->
   Result<String,String>`, dispatching by `cfg`:
   - `#[cfg(windows)]`: WIC. Extend the EXISTING `windows` dep (0.56) features with `Win32_Graphics_Imaging`
     + `Win32_System_Com` (do NOT add a 2nd windows dep). Flow: CoInitializeEx(MULTITHREADED) →
     CoCreateInstance(CLSID_WICImagingFactory) → CreateDecoderFromFilename(GENERIC_READ,CacheOnDemand) →
     GetFrame(0) → CreateFormatConverter → Initialize(GUID_WICPixelFormat32bppRGBA) → GetSize →
     CopyPixels(stride=w*4) → encode_rgba_to_png_data_url; CoUninitialize. Wrap EVERY call → Err(String),
     never panic. A missing HEIF codec (WINCODEC_ERR_COMPONENTNOTFOUND family) → clean Err (UI falls back);
     optionally a distinct error string hinting "install HEIF Image Extensions".
   - `#[cfg(target_os="macos")]`: ImageIO via `objc2`/`objc2-foundation`/`objc2-core-foundation`/
     `objc2-core-graphics`/`objc2-image-io` in a `[target.'cfg(target_os="macos")'.dependencies]` block.
     Flow: CFURL → CGImageSourceCreateWithURL → CGImageSourceCreateImageAtIndex(0) → CGBitmapContext(RGBA,
     PremultipliedLast) → CGContextDrawImage → bytes → encode_rgba_to_png_data_url. Any None/err → Err.
   - `#[cfg(not(any(windows,target_os="macos")))]`: `Err("HEIC preview not supported on this platform")`.
3. Thin `#[tauri::command] async fn read_heic_preview_data_url(path:String)` → `spawn_blocking`
   (after `ensure_previewable_size`) into `heic_preview::decode_heic_preview`. Register in `generate_handler!`
   (both sites) + regen bindings.

## Frontend

- provider.ts: `HEIC_EXT = new Set(["heic","heif","hif"])` + a provider (reuse the raw-image/decoded-image
  shape) placed before generic image; route `.heic/.heif/.hif` → `read_heic_preview_data_url`; on Err fall
  back to metadata (this is the common case when the Windows HEIF extension isn't installed — so the fallback
  must be clean and ideally show the hint). Cancel-on-selection-change. loaders/PreviewPane wiring like RAW.

## Acceptance criteria

- Windows: a real `.heic` decodes to a PNG data-URL and renders **on this dev machine** (HEIF Image Extensions
  are installed here — verify end-to-end, not just the Err path); a machine WITHOUT the extension → clean Err
  → metadata fallback (no panic/hang). Corrupt/oversized → Err.
- macOS path compiles + unit-tests on CI's macOS runner (visual correctness = attended, on a Mac).
- Backend unit test written to be green in BOTH codec-present and codec-absent environments (don't ASSERT
  decode success; assert no-panic + if Ok, URL is `data:image/png;base64,` and base64 loads via
  `image::load_from_memory` to non-zero dims).
- `cargo clippy --all-targets -- -D warnings` (src-tauri + crates/server, both modes) green; `npm run check`
  clean; JS suite green; bindings regenerated (drift guard passes). All new crates permissive (verified in
  Library entry). No bundled C, no LGPL/AGPL.

## Notes

Sequence AFTER CPE-1350 (shares src-tauri/lib.rs, bindings.gen.ts, provider.ts, loaders.ts, PreviewPane).
This UNBLOCKS/супerseeds the old libheif blocker on CPE-097. macOS visual + a no-extension-Windows-box test
are the attended remainders.

## Work Log
- 2026-08-05 (sprint): PR #646 merged (67915f02). HEIC/HEIF preview via per-OS platform APIs: Windows WIC (real decode verified on this machine - HEIF ext installed) + macOS ImageIO (cfg-gated, CI-compiled, visual attended). Shared cpe-server encode_rgba_to_png_data_url. Worker(opus); opus Reviewer APPROVE (crafted a real HEIC, proved WIC end-to-end; verified COM lifecycle + buffer safety) + UAT PASS (real 64x64 decode). Incidents fixed within-shift: macOS objc2 0.3 deprecations under -D warnings (renamed to method-form APIs); dimension guard added (review nit: bound buffer < u32::MAX). No bundled C, all-permissive licenses. macOS visual + a no-HEIF-extension Windows box = attended remainders.
