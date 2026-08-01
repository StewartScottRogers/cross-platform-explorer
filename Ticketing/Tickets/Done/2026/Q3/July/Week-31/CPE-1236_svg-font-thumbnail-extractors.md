---
id: CPE-1236
title: "SVG + font glyph-sheet thumbnail extractors (cpe-server)"
type: Task
priority: Medium
component: cpe-server
tags: [ready]
estimate: 3h
created: 2026-08-01
epic: CPE-718
closed:
---

## Context
The image thumbnail path is complete + cached, but there are NO per-format extractors for SVG or fonts
(grep-confirmed zero svg/font handling in `thumbnail.rs`/`thumb_source.rs`). Add both — they're the
lightest-dep, cleanly-headless formats. Extend the existing thumbnail-source dispatch (do NOT add a
parallel pipeline).

## Acceptance criteria
- **SVG**: rasterize an `.svg` to a PNG thumbnail at the requested max edge (resvg/usvg — lighter than a
  browser engine). Handle malformed SVG gracefully (fall back, never panic). Bomb-guard oversized SVGs
  (mirror the existing decompression-bomb guard in thumb_source.rs / CPE-1087).
- **Fonts**: render a representative glyph-sheet thumbnail for `.ttf`/`.otf`/`.woff`/`.woff2` (e.g. a few
  sample glyphs / "Aa" specimen) via a light font raster crate (ab_glyph/fontdue). Malformed font → fall
  back, no panic.
- Both integrate into the SAME dispatch the raster path uses (by extension/magic), cache via the existing
  `thumb_cache` key, and honor the existing max-edge/orientation conventions where applicable.
- REAL cargo tests: a known SVG → non-empty PNG of expected dimensions; a known font → non-empty glyph
  sheet; malformed inputs → graceful fallback (no panic). `cargo test -p cpe-server` + `cd src-tauri &&
  cargo test` + clippy both feature modes.
- If a specta::Type struct changes, regen `bindings.gen.ts`. New deps justified + minimal (prefer
  pure-Rust resvg/usvg + ab_glyph; NO heavy native/system deps).

## Notes
Cohesive backend slice (SVG+font share the dispatch + Cargo edits, so one ticket). Video/PDF are
explicitly OUT (deferred CPE-1238). Prereq for CPE-1237 (frontend streaming client).
