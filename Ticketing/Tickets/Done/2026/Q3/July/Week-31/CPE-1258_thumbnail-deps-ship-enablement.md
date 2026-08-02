---
id: CPE-1258
title: "Ship-enablement + CI + docs for PDF/video thumbnails"
type: chore
component: build
priority: medium
status: Done
tags: ready
created: 2026-08-02
epic: CPE-718
---

## Summary
Slice 3 of CPE-1238. Turn on the `pdf-thumb` + `video-thumb` features for the shipped build, add per-feature CI,
wire the pdfium/ffmpeg binary acquisition into the release build, and document the new formats. Depends on
CPE-1256 + CPE-1257 landing.

## Build
- `src-tauri/Cargo.toml`: add `pdf-thumb`, `video-thumb` to the `cpe-server` dep `features = [...]` (where `index` is enabled).
- `.github/workflows/ci.yml`: add per-feature `cargo clippy --all-targets --features pdf-thumb,video-thumb -D warnings`
  + `cargo test --features pdf-thumb,video-thumb` lines (mirror the `index`/`jwt` blocks ~ci.yml:167-178). Ensure the
  Linux/macOS/Windows runners install ffmpeg + a pdfium prebuilt so the real-render tests actually run in CI.
- `.github/workflows/release-sidecar.yml`: acquire + stage the pdfium shared lib (bblanchon/pdfium-binaries) and a
  minimal LGPL ffmpeg binary per platform as bundle resources, and carry ffmpeg's LICENSE into the distributed bundle.
- Docs (CPE-579): add/extend the thumbnails doc page in `src/docs/*.md` + its `sectionDocs.ts` entry to cover PDF +
  video thumbnail support (and that they're feature-gated / degrade to an icon when unavailable).

## Acceptance criteria
- The shipped build compiles with both features on; base/feature-off build still compiles with zero PDF/video code.
- CI runs the per-feature clippy+test on all 3 OSes with the native deps installed.
- Release bundle carries the pdfium lib + ffmpeg binary + ffmpeg LICENSE per platform.
- `sectionDocs.test.ts` stays green (every section mapped, slug exists).

## Notes
Licensing: pdfium BSD links in-process; ffmpeg stays a separate bundled exe (mere aggregation) — carry its LICENSE.
Never link mupdf (AGPL). Full end-to-end bundling is verifiable only once this lands + CI runs (CI currently
intermittently stalled — verify what can be verified locally).

## Work Log
- 2026-08-02 — Worker (sonnet, worktree) enabled pdf-thumb+video-thumb in src-tauri, per-feature CI (ffmpeg/pdfium
  installs), release-sidecar.yml native-dep staging (pdfium chromium/7961, LGPL ffmpeg n8.1.2, LICENSE carried), pdfium
  overlay confs, docs. Opus Reviewer caught a BLOCKING defect: resolvers used current_exe().parent() but Tauri stages
  resources to resource_dir() (differs on macOS/Linux) → silent icon fallback; CI couldn't catch it (only tests the
  PATH fallback). Fix (c79fa0a8): inject app.path().resource_dir() into cpe-server via set_native_dep_dir setters
  (mirrors resolve_sidecar_bin), called from setup(). Reviewer re-verified → APPROVE. Merged #560 (df7f65b3, --admin).
- Full bundled-path end-to-end confirmation needs a real macOS/Linux release build (attended/CI-gated).
