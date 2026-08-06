---
id: CPE-1349
title: "Wire Camera-RAW preview provider: read_raw_preview_data_url command + cr2/nef/arw decoded-image provider"
type: Feature
status: Backlog
priority: Low
component: Multiple
tags: [ready]
epic: CPE-102
created: 2026-08-05
closed:
---

## Goal

Make `.cr2`/`.nef`/`.arw` show their embedded JPEG preview in the pane, using the backend just landed
(CPE-1346, `cpe_server::camera_raw::read_raw_preview_data_url` → a `data:image/jpeg;base64,...` URL).
Mostly headless (jsdom/vitest); the only eyes-on part is confirming the image renders (low-risk `<img>`).

## Changes (mirror the existing `decoded-image` / `read_image_data_url` path for tiff/psd)

1. **Backend command** — `src-tauri/src/lib.rs`: add a thin `#[tauri::command] async fn
   read_raw_preview_data_url(path: String) -> Result<String, String>` that `spawn_blocking`s into
   `cpe_server::camera_raw::read_raw_preview_data_url(&path)` (mirror `read_image_data_url` at ~line 937).
   Register it in the `generate_handler!` macro.
2. **Bindings** — regenerate `src/lib/bindings.gen.ts` (specta) so the typed `read_raw_preview_data_url`
   wrapper exists. (Run the repo's binding-export step; verify the Typed-bindings drift guard would pass —
   this is a real CI gate, see [[regen-specta-bindings-on-struct-change]].)
3. **Frontend provider** — `src/lib/preview/provider.ts`: register cr2/nef/arw so they route to a decoded
   preview. Add a `RAW_EXT = new Set(["cr2","nef","arw"])` and a provider (place it before the generic
   `image` provider, like `decoded-image` is) whose `canPreview` matches RAW_EXT. Reuse the `decoded-image`
   kind or add a `raw-image` kind — whichever integrates most cleanly with the loader.
4. **Loader** — wherever `decoded-image` invokes `read_image_data_url` (the loader / PreviewPane path):
   branch so cr2/nef/arw call `read_raw_preview_data_url` instead, tiff/psd keep `read_image_data_url`.
5. **Graceful fallback** — a RAW file with no embedded JPEG (backend returns `Err`) must fall back to the
   metadata/info view, not error the pane. In-flight loads cancelled on selection change (reuse the existing
   cancellation the image provider already has).

## Acceptance criteria

- Selecting a `.cr2`/`.nef`/`.arw` requests `read_raw_preview_data_url` and renders the returned JPEG; an
  Err (no embedded preview) falls back to metadata without breaking the pane.
- jsdom/vitest: the provider matches cr2/nef/arw and routes to the raw command (mock the invoke); existing
  provider tests still pass. `npm run check` clean; JS suite green.
- Backend: `cargo clippy --all-targets -- -D warnings` (both modes) green; `read_raw_preview_data_url`
  command compiles + is registered. Bindings regenerated (no drift-guard failure).
- No new deps.

## Notes

Touches `src-tauri/src/lib.rs`, `bindings.gen.ts`, `src/lib/preview/provider.ts`, the loader, + tests.
Do this on ONE branch (shared frontend files — do not parallelize with the DICOM provider wiring). The
final "does the image look right" is a quick attended/Visual-Critic check after merge.
