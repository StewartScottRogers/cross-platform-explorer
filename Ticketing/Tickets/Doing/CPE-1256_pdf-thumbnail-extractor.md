---
id: CPE-1256
title: "PDF first-page thumbnail extractor (pdfium-render, feature-gated)"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-02
epic: CPE-718
---

## Summary
Slice 1 of CPE-1238 (dep-approach decided — see research-library `thumbnail-native-deps-pdf-video-2026-08-02.md`).
Add a **PDF first-page → thumbnail** extractor, in-process, behind a new off-by-default `pdf-thumb` feature,
using `pdfium-render` (MIT/Apache) + the dynamically-loaded pdfium prebuilt (BSD-3-Clause). Mirrors the
`thumb_svg.rs` extractor pattern and plugs into `decode_thumb_image`.

## Build
- New `crates/server/src/thumb_pdf.rs`: `pub fn render_first_page(bytes: &[u8], max_edge: u32) -> Result<DynamicImage, String>`.
  Mirror `thumb_svg.rs`: self-contained, **bomb-guard** the rendered page dimensions before allocating a canvas,
  never panic (malformed/empty/encrypted PDF → `Err`). Render page 0 to a bitmap at ~`max_edge` longest side.
- Load pdfium via `Pdfium::bind_to_library(...)` resolving a bundled lib path, with a clear `Err` when the lib
  is absent (graceful — pipeline shows the type icon).
- `crates/server/Cargo.toml`: add `pdfium-render` as an **optional** dep + `[features] pdf-thumb = ["dep:pdfium-render"]` (off by default).
- `crates/server/src/lib.rs`: add `#[cfg(feature="pdf-thumb")] pub mod thumb_pdf;`.
- `crates/server/src/thumb_source.rs`: add ONE `#[cfg(feature="pdf-thumb")] "pdf" => thumb_pdf::render_first_page(&bytes, max_edge)?` arm alongside the svg/psd arms.
- Cargo tests: render a **minimal single-page PDF fixture** → assert a non-degenerate `DynamicImage`; malformed
  bytes → `Err` (no panic). If pdfium lib isn't present in the test env, gate the real-render test behind a
  presence/env check (document it) so the suite stays green cross-OS; keep the malformed/`Err` test unconditional.

## Acceptance criteria
- `cargo build`/`test`/`clippy --all-targets -D warnings` clean **both** with and without `--features pdf-thumb`
  (feature-off compiles zero PDF code — the "small when off" rule).
- `decode_thumb_image` renders a PDF first page when the feature is on + lib present; falls through to Err→icon otherwise.
- No new dep pulled into the feature-off build. Bindings unaffected (no specta struct change).
- Regen `bindings.gen.ts` only if a specta::Type struct changed (it shouldn't).

## Notes
Sibling of CPE-1257 (video, next). Both share additive lines in thumb_source.rs/Cargo.toml/lib.rs — sequenced
after this to avoid a merge. pdfium is NOT installed locally; ffmpeg IS (for the sibling). Ship-enablement +
CI + release binary acquisition is CPE-1258.
